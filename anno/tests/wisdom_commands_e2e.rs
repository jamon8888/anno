//! End-to-end tests for Wisdom-inspired CLI commands.
//!
//! These tests verify the new commands added based on the "wisdoms" document:
//! - `privacy` (Wisdom 38: Redaction/Privacy Problem)
//! - `domain` (Wisdom 37: Domain Shift Indicator)
//! - `singleton` (Wisdom 28: Singleton Cluster Problem)
//! - `context` (Wisdom 33: Context Window Export)

use std::process::Command;

fn get_binary_path() -> Option<&'static str> {
    if std::path::Path::new("target/release/anno").exists() {
        Some("target/release/anno")
    } else if std::path::Path::new("target/debug/anno").exists() {
        Some("target/debug/anno")
    } else {
        None
    }
}

// =============================================================================
// Privacy Command Tests (Wisdom 38)
// =============================================================================

#[test]
fn test_privacy_report_basic() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "privacy",
            "--text",
            "Dr. John Smith lives at 123 Main St and can be reached at john@example.com",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("PII Detection Report"));
    assert!(stdout.contains("PERSON"));
}

#[test]
fn test_privacy_report_detects_email() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "privacy",
            "--text",
            "Contact john.smith@example.com for info",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("CONTACT") || stdout.contains("EMAIL"));
}

#[test]
fn test_privacy_redact_action() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "privacy",
            "--text",
            "Dr. John Smith met Jane Doe",
            "--action",
            "redact",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should contain replacement tokens
    assert!(stdout.contains("[PERSON_"));
}

#[test]
fn test_privacy_pseudonymize_action() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "privacy",
            "--text",
            "Dr. John Smith met Jane Doe",
            "--action",
            "pseudonymize",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should NOT contain the original names
    assert!(!stdout.contains("John Smith"));
}

#[test]
fn test_privacy_quiet_mode() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&["privacy", "--text", "Dr. John Smith", "--quiet"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should be TSV-like compact output
    assert!(!stdout.contains("PII Detection Report"));
}

#[test]
fn test_privacy_type_filter() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "privacy",
            "--text",
            "Dr. John Smith, john@example.com, March 15, 1985",
            "--types",
            "PERSON",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("PERSON"));
}

// =============================================================================
// Domain Command Tests (Wisdom 37)
// =============================================================================

#[test]
fn test_domain_general_text() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "domain",
            "--text",
            "Marie Curie won the Nobel Prize in Paris.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Domain Analysis Report"));
    assert!(stdout.contains("Detected Domain"));
}

#[test]
fn test_domain_biomedical_text() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "domain",
            "--text",
            "The patient presented with symptoms of hypertension. Clinical diagnosis revealed elevated blood pressure. Treatment includes medication.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("biomedical"));
    assert!(stdout.contains("High") || stdout.contains("HIGH"));
}

#[test]
fn test_domain_legal_text() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "domain",
            "--text",
            "The plaintiff hereby files motion pursuant to the court's jurisdiction. The defendant shall appear.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("legal"));
}

#[test]
fn test_domain_json_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "domain",
            "--text",
            "The company reported strong revenue and earnings growth.",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["detected_domain"].is_string());
    assert!(json["shift_risk"].is_string());
}

#[test]
fn test_domain_quiet_mode() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "domain",
            "--text",
            "General news article about events.",
            "--quiet",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should be compact TSV-like output
    assert!(!stdout.contains("Domain Analysis Report"));
}

// =============================================================================
// Singleton Command Tests (Wisdom 28)
// =============================================================================

#[test]
fn test_singleton_basic() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "singleton",
            "--text",
            "Dr. John Smith met with Jane Doe. The doctor examined the patient.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Singleton Analysis Report"));
    assert!(stdout.contains("Total entities"));
}

#[test]
fn test_singleton_verbose_shows_all() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "singleton",
            "--text",
            "John Smith met Jane. John was pleased.",
            "--verbose",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("All Singletons") || stdout.contains("Likely"));
}

#[test]
fn test_singleton_json_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "singleton",
            "--text",
            "Marie Curie discovered radium.",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["total_entities"].is_number());
    assert!(json["singleton_count"].is_number());
}

#[test]
fn test_singleton_tsv_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "singleton",
            "--text",
            "Marie Curie discovered radium.",
            "--format",
            "tsv",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("text\ttype\tstart\tend"));
}

#[test]
fn test_singleton_quiet_mode() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&["singleton", "--text", "Dr. John Smith", "--quiet"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("Singleton Analysis Report"));
}

// =============================================================================
// Context Command Tests (Wisdom 33)
// =============================================================================

#[test]
fn test_context_basic() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Dr. Marie Curie from Paris won the Nobel Prize in 1903.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Entity Context Export"));
    assert!(stdout.contains("Context:"));
}

#[test]
fn test_context_full_sentence() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Dr. Marie Curie won the Nobel Prize. She was born in Poland.",
            "--full-sentence",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Sentence:"));
}

#[test]
fn test_context_json_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Dr. Marie Curie won the Nobel Prize.",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["entities"].is_array());
    assert!(json["count"].is_number());

    // Check entity has context fields
    if let Some(first) = json["entities"].as_array().and_then(|a| a.first()) {
        assert!(first["context"]["left"].is_string());
        assert!(first["context"]["right"].is_string());
    }
}

#[test]
fn test_context_tsv_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Marie Curie discovered radium.",
            "--format",
            "tsv",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("text\ttype\tstart\tend"));
    assert!(stdout.contains("left_context"));
    assert!(stdout.contains("right_context"));
}

#[test]
fn test_context_markdown_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Marie Curie discovered radium.",
            "--format",
            "markdown",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("# Entity Context Export"));
    assert!(stdout.contains("|"));
}

#[test]
fn test_context_brat_output() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Marie Curie discovered radium.",
            "--format",
            "brat",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // brat format: T1\tType Start End\tText
    assert!(stdout.starts_with("T1\t"));
}

#[test]
fn test_context_window_size() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "The famous Dr. Marie Curie from Paris won the Nobel Prize in Physics and Chemistry.",
            "--window",
            "100",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // With window=100, context should capture more text
    if let Some(first) = json["entities"].as_array().and_then(|a| a.first()) {
        let left = first["context"]["left"].as_str().unwrap_or("");
        let right = first["context"]["right"].as_str().unwrap_or("");
        // Context should have substantial text
        assert!(left.len() + right.len() > 0);
    }
}

#[test]
fn test_context_entity_type_filter() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Dr. Marie Curie from Paris won the Nobel Prize.",
            "--entity-type",
            "PER",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // All entities should be PER type
    if let Some(entities) = json["entities"].as_array() {
        for entity in entities {
            assert_eq!(entity["entity"]["type"].as_str(), Some("PER"));
        }
    }
}

#[test]
fn test_context_quiet_mode() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary)
        .args(&[
            "context",
            "--text",
            "Marie Curie discovered radium.",
            "--quiet",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("Entity Context Export"));
}

// =============================================================================
// Cross-Command Integration Tests
// =============================================================================

#[test]
fn test_all_wisdom_commands_have_help() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    for cmd in &["privacy", "domain", "singleton", "context"] {
        let output = Command::new(binary)
            .args(&[cmd, "--help"])
            .output()
            .unwrap();

        assert!(output.status.success(), "{} --help should succeed", cmd);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("Usage:"),
            "{} --help should contain Usage",
            cmd
        );
    }
}

#[test]
fn test_all_wisdom_commands_handle_empty_input_gracefully() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    for cmd in &["privacy", "domain", "singleton", "context"] {
        let output = Command::new(binary).args(&[cmd]).output().unwrap();

        // Should fail but not crash
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("No input") || stderr.contains("input") || !output.status.success(),
            "{} should handle missing input",
            cmd
        );
    }
}

#[test]
fn test_wisdom_commands_listed_in_help() {
    let Some(binary) = get_binary_path() else {
        return;
    };

    let output = Command::new(binary).args(&["--help"]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // All wisdom commands should appear in main help
    assert!(
        stdout.contains("privacy"),
        "privacy command should be in --help"
    );
    assert!(
        stdout.contains("domain"),
        "domain command should be in --help"
    );
    assert!(
        stdout.contains("singleton"),
        "singleton command should be in --help"
    );
    assert!(
        stdout.contains("context"),
        "context command should be in --help"
    );
}
