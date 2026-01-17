#[cfg(test)]
#[cfg(feature = "eval-advanced")]
mod tests {
    use anno::eval::config_builder::TaskEvalConfigBuilder;
    use anno::eval::loader::DatasetId;
    use anno::eval::task_evaluator::TaskEvaluator;

    /// Test that backend initialization failures are properly reported.
    #[test]
    fn test_backend_init_failure_reporting() {
        let evaluator = TaskEvaluator::new().expect("Failed to create evaluator");

        // Try to evaluate with a backend that may fail to initialize
        let config = TaskEvalConfigBuilder::new()
            .with_backends(vec!["gliner".to_string()])
            .with_datasets(vec![DatasetId::WikiGold])
            .with_max_examples(1)
            .with_seed(42)
            .build();

        let results = evaluator
            .evaluate_all(config)
            .expect("Evaluation should complete");

        // Check that failures are properly reported
        let gliner_results: Vec<_> = results
            .results
            .iter()
            .filter(|r| r.backend == "gliner")
            .collect();

        assert!(
            !gliner_results.is_empty(),
            "Should have at least one gliner result"
        );

        for result in gliner_results {
            if !result.success {
                // Failed results should have error messages
                assert!(
                    result.error.is_some(),
                    "Failed result should have error message: {:?}",
                    result
                );
                let error_msg = result.error.as_ref().unwrap();
                // Error should be informative
                assert!(!error_msg.is_empty(), "Error message should not be empty");
                println!("✓ gliner failure properly reported: {}", error_msg);
            } else {
                println!("✓ gliner evaluation succeeded");
            }
        }
    }

    /// Test that empty entity results are handled correctly.
    #[test]
    fn test_empty_entity_handling() {
        let evaluator = TaskEvaluator::new().expect("Failed to create evaluator");

        // Use a backend that might return empty results
        let config = TaskEvalConfigBuilder::new()
            .with_backends(vec!["pattern".to_string()]) // Pattern only extracts structured entities
            .with_datasets(vec![DatasetId::WikiGold]) // WikiGold has named entities, not structured
            .with_max_examples(5)
            .with_seed(42)
            .build();

        let results = evaluator
            .evaluate_all(config)
            .expect("Evaluation should complete");

        let pattern_results: Vec<_> = results
            .results
            .iter()
            .filter(|r| r.backend == "pattern")
            .collect();

        assert!(!pattern_results.is_empty(), "Should have pattern results");

        for result in pattern_results {
            // Even if no entities are extracted, evaluation should complete
            assert!(
                result.duration_ms.is_some() || result.error.is_some(),
                "Result should have duration or error: {:?}",
                result
            );

            if let Some(metrics) = result.metrics.get("f1") {
                // F1 might be 0.0 if no entities match, but that's valid
                assert!(
                    *metrics >= 0.0 && *metrics <= 100.0,
                    "F1 should be in [0, 100]"
                );
            }
        }
    }

    /// Test that incompatible backend-dataset combinations are marked correctly.
    #[test]
    fn test_incompatible_backend_dataset() {
        let evaluator = TaskEvaluator::new().expect("Failed to create evaluator");

        // heuristic only supports PER/ORG/LOC, not biomedical types
        let config = TaskEvalConfigBuilder::new()
            .with_backends(vec!["heuristic".to_string()])
            .with_datasets(vec![DatasetId::BC5CDR]) // BC5CDR has Disease, Chemical
            .with_max_examples(5)
            .with_seed(42)
            .build();

        let results = evaluator
            .evaluate_all(config)
            .expect("Evaluation should complete");

        let heuristic_results: Vec<_> = results
            .results
            .iter()
            .filter(|r| r.backend == "heuristic" && r.dataset == DatasetId::BC5CDR)
            .collect();

        assert!(
            !heuristic_results.is_empty(),
            "Should have heuristic/BC5CDR result"
        );

        for result in heuristic_results {
            // Should be marked as incompatible
            if let Some(error) = &result.error {
                assert!(
                    error.contains("incompatible") || error.contains("doesn't support"),
                    "Incompatible combination should have clear error: {}",
                    error
                );
                println!("✓ Incompatible combination properly marked: {}", error);
            }
        }
    }
}
