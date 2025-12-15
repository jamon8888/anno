//! Integration tests for the `anno dataset` CLI command.
//!
//! Tests the dataset subcommands: list, info, eval.
//!
//! ## Test Categories
//!
//! 1. **List command tests**: Verify dataset listing and filtering
//! 2. **Info command tests**: Verify dataset metadata display
//! 3. **Eval command tests**: Verify evaluation pipeline (synthetic + cached)
//!
//! ## Running
//!
//! ```bash
//! # Build release binary first for faster tests
//! cargo build --release -p anno-cli
//!
//! # Run dataset CLI tests
//! cargo test --test dataset_command_cli --features eval
//! ```

#![cfg(feature = "eval")]

use std::process::Command;

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the anno CLI command, preferring pre-built binaries.
fn anno_cli_cmd() -> Command {
    if let Ok(bin_path) = std::env::var("ANNO_CLI_BIN") {
        return Command::new(bin_path);
    }

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR should have a parent");

    // Try release binary first (fastest)
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
    cmd.args(["run", "-p", "anno-cli", "--"]);
    cmd
}

// =============================================================================
// Dataset List Command Tests
// =============================================================================

/// Test `anno dataset list` runs without error.
#[test]
fn test_dataset_list_basic() {
    let output = anno_cli_cmd()
        .args(["dataset", "list"])
        .output()
        .expect("failed to execute anno dataset list");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(
        output.status.success(),
        "dataset list failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Should mention datasets
    assert!(
        stdout.contains("Available Datasets") || stdout.contains("datasets"),
        "Output should mention datasets:\n{}",
        stdout
    );
}

/// Test `anno dataset list --loadable` shows only downloadable datasets.
#[test]
fn test_dataset_list_loadable() {
    let output = anno_cli_cmd()
        .args(["dataset", "list", "--loadable"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    // Should show loadable datasets
    assert!(
        stdout.contains("loadable") || stdout.contains("WikiGold") || stdout.contains("wikigold"),
        "Should list loadable datasets:\n{}",
        stdout
    );
}

/// Test `anno dataset list --task ner` filters to NER datasets.
#[test]
fn test_dataset_list_task_filter() {
    let output = anno_cli_cmd()
        .args(["dataset", "list", "--task", "ner", "--loadable"])
        .output()
        .expect("failed to execute");

    let _stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    // NER filter should work (might have empty output if no NER datasets)
}

/// Test `anno dataset list --verbose` shows more details.
#[test]
fn test_dataset_list_verbose() {
    let output = anno_cli_cmd()
        .args(["dataset", "list", "--loadable", "--verbose"])
        .output()
        .expect("failed to execute");

    let _stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    // Verbose output typically includes years and citations
}

// =============================================================================
// Dataset Info Command Tests
// =============================================================================

/// Test `anno dataset info --dataset wikigold` shows dataset details.
#[test]
fn test_dataset_info_wikigold() {
    let output = anno_cli_cmd()
        .args(["dataset", "info", "--dataset", "wikigold"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed (whether or not dataset is cached)
    assert!(
        output.status.success(),
        "dataset info failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Should contain dataset information
    assert!(
        stdout.to_lowercase().contains("wikigold")
            || stdout.to_lowercase().contains("dataset")
            || stdout.to_lowercase().contains("wiki"),
        "Output should mention the dataset:\n{}",
        stdout
    );
}

/// Test `anno dataset info` with unknown dataset gives error.
#[test]
fn test_dataset_info_unknown() {
    let output = anno_cli_cmd()
        .args(["dataset", "info", "--dataset", "nonexistent_dataset_xyz"])
        .output()
        .expect("failed to execute");

    // Should fail gracefully
    assert!(
        !output.status.success() || String::from_utf8_lossy(&output.stderr).contains("Unknown"),
        "Should fail or warn for unknown dataset"
    );
}

// =============================================================================
// Dataset Eval Command Tests
// =============================================================================

/// Test `anno dataset eval` with synthetic dataset.
#[test]
fn test_dataset_eval_synthetic_ner() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--model",
            "pattern",
            "--task",
            "ner",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(
        output.status.success(),
        "dataset eval synthetic failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Should show evaluation metrics
    assert!(
        stdout.contains("F1") || stdout.contains("Precision") || stdout.contains("Results"),
        "Output should contain evaluation metrics:\n{}",
        stdout
    );
}

/// Test `anno dataset eval` with pattern model (fast).
#[test]
fn test_dataset_eval_pattern_model() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--model",
            "pattern",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    // Pattern model should find something
    assert!(
        stdout.contains("Evaluating") || stdout.contains("pattern"),
        "Should mention evaluation:\n{}",
        stdout
    );
}

/// Test coref task requires real dataset.
#[test]
fn test_dataset_eval_coref_needs_real_dataset() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--task",
            "coref",
        ])
        .output()
        .expect("failed to execute");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail or warn that coref needs real dataset
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        !output.status.success() || combined.contains("requires") || combined.contains("real"),
        "Coref should require real dataset"
    );
}

/// Test relation task requires real dataset.
#[test]
fn test_dataset_eval_relation_needs_real_dataset() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--task",
            "relation",
        ])
        .output()
        .expect("failed to execute");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail or warn that relation needs real dataset
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        !output.status.success() || combined.contains("requires") || combined.contains("real"),
        "Relation should require real dataset"
    );
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

/// Test invalid model backend gives helpful error.
#[test]
fn test_dataset_eval_invalid_model() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--model",
            "invalid_backend_xyz",
        ])
        .output()
        .expect("failed to execute");

    // Should fail with an error message
    assert!(
        !output.status.success(),
        "Should fail for invalid model backend"
    );
}

/// Test invalid task gives helpful error.
#[test]
fn test_dataset_eval_invalid_task() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--task",
            "invalid_task_xyz",
        ])
        .output()
        .expect("failed to execute");

    // Should fail with an error message
    assert!(!output.status.success(), "Should fail for invalid task");
}

/// Test help works for dataset command.
#[test]
fn test_dataset_help() {
    let output = anno_cli_cmd()
        .args(["dataset", "--help"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(
        stdout.contains("list") || stdout.contains("info") || stdout.contains("eval"),
        "Help should mention subcommands:\n{}",
        stdout
    );
}

// =============================================================================
// Evaluation Invariants (Property Tests)
// =============================================================================

/// F1 score must be in [0, 1] range.
#[test]
fn test_eval_f1_range_invariant() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--model",
            "pattern",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse F1 from output (if present)
    if let Some(f1_match) = stdout.split("F1:").nth(1) {
        if let Some(f1_str) = f1_match.split('%').next() {
            let f1_str = f1_str.trim();
            if let Ok(f1) = f1_str.parse::<f64>() {
                assert!(
                    (0.0..=100.0).contains(&f1),
                    "F1 should be in [0, 100]%, got {}",
                    f1
                );
            }
        }
    }
}

/// Precision and recall must be in [0, 1] range.
#[test]
fn test_eval_pr_range_invariant() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            "synthetic",
            "--model",
            "pattern",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check P (precision)
    if let Some(p_match) = stdout.split("P:").nth(1) {
        if let Some(p_str) = p_match.split('%').next() {
            if let Ok(p) = p_str.trim().parse::<f64>() {
                assert!(
                    (0.0..=100.0).contains(&p),
                    "Precision should be in [0, 100]%, got {}",
                    p
                );
            }
        }
    }

    // Check R (recall)
    if let Some(r_match) = stdout.split("R:").nth(1) {
        if let Some(r_str) = r_match.split('%').next() {
            if let Ok(r) = r_str.trim().parse::<f64>() {
                assert!(
                    (0.0..=100.0).contains(&r),
                    "Recall should be in [0, 100]%, got {}",
                    r
                );
            }
        }
    }
}

// =============================================================================
// Coreference Evaluation Tests (if datasets available)
// =============================================================================

/// Test coref dataset evaluation if cached (any available coref dataset).
#[test]
#[cfg(feature = "eval-advanced")]
fn test_coref_eval_if_cached() {
    use anno::eval::loader::{DatasetId, DatasetLoader};
    use anno::eval::LoadableDatasetId;

    let loader = match DatasetLoader::new() {
        Ok(l) => l,
        Err(_) => {
            eprintln!("Skipping: DatasetLoader not available");
            return;
        }
    };

    // Find any cached coref dataset
    let coref_datasets = DatasetId::all_coref();
    let cached_coref = coref_datasets
        .iter()
        .find(|id| match LoadableDatasetId::try_from(***id) {
            Ok(loadable) => loader.is_cached(loadable),
            Err(_) => false,
        });

    let dataset_id = match cached_coref {
        Some(id) => *id,
        None => {
            eprintln!("No coref datasets cached - skipping coref eval test");
            return;
        }
    };

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "eval",
            "--dataset",
            dataset_id.name(),
            "--task",
            "coref",
            "--model",
            "pattern",
        ])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        // Should show CoNLL metrics
        assert!(
            stdout.contains("CoNLL")
                || stdout.contains("MUC")
                || stdout.contains("B³")
                || stdout.contains("CEAF"),
            "Coref output should show metrics:\nstdout: {}\nstderr: {}",
            stdout,
            stderr
        );
    } else {
        // May fail if eval-advanced not enabled for the specific dataset
        eprintln!(
            "{} eval failed (may need eval-advanced): {}",
            dataset_id.name(),
            stderr
        );
    }
}

// =============================================================================
// Cross-Validation with External Scorers (Reference)
// =============================================================================

/// Document: Anno's coreference metrics should align with scorch (Python scorer).
///
/// Reference implementations for validation:
/// - `LoicGrobol/scorch`: MIT licensed Python coreference scorer
///   <https://github.com/LoicGrobol/scorch>
/// - Implements MUC, B³, CEAF-e, BLANC per CoNLL-2011/2012
///
/// To cross-validate:
/// 1. Export Anno's predictions to scorch JSON format
/// 2. Run scorch scorer
/// 3. Compare metrics
///
/// This is left as a TODO for comprehensive validation.
#[test]
fn test_cross_validation_reference_doc() {
    // This test documents the cross-validation approach.
    // The actual implementation would require:
    // 1. JSON export of Anno predictions
    // 2. Python subprocess to run scorch
    // 3. Metric comparison
    //
    // For now, just verify we have the metrics defined.
    #[cfg(feature = "eval-advanced")]
    {
        use anno::eval::coref_metrics::AggregateCorefEvaluation;
        // Type exists
        let _: fn() = || {
            let _: Option<AggregateCorefEvaluation> = None;
        };
    }
}
