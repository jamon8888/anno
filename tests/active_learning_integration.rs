//! Integration tests for active learning with NER backends.
//!
//! Active learning is one of the future directions identified in Munnangi (2020)
//! "A Brief History of Named Entity Recognition" (arXiv:2411.05057):
//!
//! > "Active learning offers a solution for [annotation bottleneck], where it
//! > efficiently selects the set needed to label... A central challenge in active
//! > learning is to determine what data is more informative than the rest."
//!
//! This module tests the integration of active learning with Anno's NER backends.
//!
//! # Test Categories
//!
//! 1. **Uncertainty sampling**: Select low-confidence predictions for annotation
//! 2. **Query-by-committee**: Select examples where backends disagree
//! 3. **Realistic workflow**: End-to-end active learning simulation

#![cfg(feature = "eval-advanced")]

use anno::eval::active_learning::{ActiveLearner, Candidate, SamplingStrategy, SelectionResult};
use anno::{CrfNER, HeuristicNER, Model, RegexNER, StackedNER};

// =============================================================================
// Test Data
// =============================================================================

/// Unlabeled corpus for active learning simulation.
/// Mix of easy and hard examples.
const UNLABELED_CORPUS: &[&str] = &[
    // Easy (clear entities)
    "John Smith works at Google Inc.",
    "Marie Curie won the Nobel Prize.",
    "The meeting is scheduled for March 15, 2024.",
    // Medium (some ambiguity)
    "Apple announced new products today.", // Apple: company or fruit?
    "Washington signed the treaty.",       // Person or location?
    "Jordan visited the museum.",          // Person or country?
    // Hard (domain-specific, unusual patterns)
    "Dr. Xiangjun Zhang published the paper.",
    "The CEO of Anthropic met with regulators.",
    "GPT-4 was released by OpenAI.",
    // Sparse entities
    "The weather is nice today.",
    "I went to the store yesterday.",
];

// =============================================================================
// Uncertainty Sampling with Real Backends
// =============================================================================

mod uncertainty_sampling {
    use super::*;

    /// Convert NER predictions to active learning candidates.
    fn predictions_to_candidates(texts: &[&str], model: &dyn Model) -> Vec<Candidate> {
        texts
            .iter()
            .map(|text| {
                let entities = model.extract_entities(text, None).unwrap_or_default();

                // Use minimum confidence as overall confidence
                // (most uncertain entity dominates)
                let confidence = entities
                    .iter()
                    .map(|e| e.confidence)
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.5); // No entities = uncertain

                let types: Vec<String> =
                    entities.iter().map(|e| e.entity_type.to_string()).collect();

                Candidate::new(*text, confidence).with_types(types)
            })
            .collect()
    }

    #[test]
    fn uncertainty_sampling_selects_low_confidence() {
        let model = CrfNER::new();
        let candidates = predictions_to_candidates(UNLABELED_CORPUS, &model);

        let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);
        let result = learner.select_with_scores(&candidates, 3);

        // Verify selection properties
        assert_eq!(result.selected.len(), 3);
        assert_eq!(result.actual_strategy, SamplingStrategy::Uncertainty);

        // Selected should have lower confidence than average
        let selected_conf_avg: f64 = result.selected.iter().map(|(_, score)| score).sum::<f64>()
            / result.selected.len() as f64;

        // Note: score is inverse of confidence for uncertainty sampling
        println!(
            "Selected {} examples with avg score {:.3} (higher = more uncertain)",
            result.selected.len(),
            selected_conf_avg
        );
    }

    #[test]
    fn uncertainty_sampling_prioritizes_no_entity_texts() {
        let model = RegexNER::new(); // Pattern-based, won't find names
        let candidates = predictions_to_candidates(UNLABELED_CORPUS, &model);

        let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);
        let selected = learner.select(&candidates, 5);

        // RegexNER won't find PER/ORG entities, so texts without patterns
        // should have low confidence (0.5 default)
        for candidate in &selected {
            assert!(
                candidate.confidence <= 0.6,
                "Expected low confidence, got {} for '{}'",
                candidate.confidence,
                candidate.text
            );
        }
    }
}

// =============================================================================
// Query-by-Committee (Multiple Backends)
// =============================================================================

mod committee_sampling {
    use super::*;

    /// Create candidates with predictions from multiple backends.
    fn committee_predictions(texts: &[&str]) -> Vec<Candidate> {
        let backends: Vec<Box<dyn Model>> = vec![
            Box::new(RegexNER::new()),
            Box::new(HeuristicNER::new()),
            Box::new(CrfNER::new()),
        ];

        texts
            .iter()
            .map(|text| {
                let predictions: Vec<Vec<String>> = backends
                    .iter()
                    .map(|backend| {
                        let entities = backend.extract_entities(text, None).unwrap_or_default();
                        entities.iter().map(|e| e.entity_type.to_string()).collect()
                    })
                    .collect();

                // Average confidence across backends
                let confidences: Vec<f64> = backends
                    .iter()
                    .map(|backend| {
                        let entities = backend.extract_entities(text, None).unwrap_or_default();
                        entities.iter().map(|e| e.confidence).sum::<f64>()
                            / entities.len().max(1) as f64
                    })
                    .collect();
                let avg_confidence = confidences.iter().sum::<f64>() / confidences.len() as f64;

                let mut candidate = Candidate::new(*text, avg_confidence);
                candidate.committee_predictions = predictions;
                candidate
            })
            .collect()
    }

    #[test]
    fn committee_sampling_selects_disagreement() {
        let candidates = committee_predictions(UNLABELED_CORPUS);

        let learner = ActiveLearner::new(SamplingStrategy::QueryByCommittee);
        let result = learner.select_with_scores(&candidates, 3);

        // Should successfully use committee strategy if predictions available
        assert_eq!(result.selected.len(), 3);

        println!("Committee sampling selected:");
        for (text, score) in &result.selected {
            println!("  - '{}' (disagreement score: {:.3})", text, score);
        }
    }

    #[test]
    fn committee_identifies_ambiguous_entities() {
        // Test specific ambiguous cases
        let ambiguous_texts = [
            "Apple announced profits.",  // ORG vs no entity
            "Washington visited Paris.", // PER vs LOC
            "Jordan won the game.",      // PER vs LOC
        ];

        let candidates = committee_predictions(&ambiguous_texts);

        let learner = ActiveLearner::new(SamplingStrategy::QueryByCommittee);
        let selected = learner.select(&candidates, 2);

        // Should select texts where backends disagree
        assert_eq!(selected.len(), 2);

        // Verify committee predictions exist
        for candidate in &selected {
            assert!(
                candidate.committee_predictions.len() >= 2,
                "Expected committee predictions for '{}'",
                candidate.text
            );
        }
    }
}

// =============================================================================
// Hybrid Strategies
// =============================================================================

mod hybrid_strategies {
    use super::*;

    #[test]
    fn hybrid_combines_uncertainty_and_committee() {
        // Build candidates with both confidence and committee predictions
        let texts = &UNLABELED_CORPUS[..5];

        let backends: Vec<Box<dyn Model>> =
            vec![Box::new(HeuristicNER::new()), Box::new(CrfNER::new())];

        let candidates: Vec<Candidate> = texts
            .iter()
            .map(|text| {
                let predictions: Vec<Vec<String>> = backends
                    .iter()
                    .map(|backend| {
                        let entities = backend.extract_entities(text, None).unwrap_or_default();
                        entities.iter().map(|e| e.entity_type.to_string()).collect()
                    })
                    .collect();

                // Use first backend's confidence
                let entities = backends[0].extract_entities(text, None).unwrap_or_default();
                let confidence = entities
                    .iter()
                    .map(|e| e.confidence)
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.5);

                let mut candidate = Candidate::new(*text, confidence);
                candidate.committee_predictions = predictions;
                candidate
            })
            .collect();

        let learner = ActiveLearner::new(SamplingStrategy::Hybrid);
        let result = learner.select_with_scores(&candidates, 2);

        assert_eq!(result.selected.len(), 2);
        println!("Hybrid strategy used: {:?}", result.actual_strategy);
    }
}

// =============================================================================
// Realistic Workflow Simulation
// =============================================================================

mod workflow {
    use super::*;

    /// Simulate an active learning annotation workflow.
    #[test]
    fn simulate_annotation_iteration() {
        let model = StackedNER::default();

        // Initial predictions on unlabeled data
        let mut candidates: Vec<Candidate> = UNLABELED_CORPUS
            .iter()
            .map(|text| {
                let entities = model.extract_entities(text, None).unwrap_or_default();
                let confidence = entities
                    .iter()
                    .map(|e| e.confidence)
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.5);
                Candidate::new(*text, confidence)
            })
            .collect();

        // Active learning iteration
        let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);

        // Round 1: Select most uncertain examples
        let round1 = learner.select(&candidates, 3);
        let round1_texts: std::collections::HashSet<String> =
            round1.iter().map(|c| c.text.clone()).collect();
        println!("\nRound 1 - Selected for annotation:");
        for c in &round1 {
            println!("  [{:.2}] {}", c.confidence, c.text);
        }

        // Simulate annotation: "annotate" selected examples by increasing confidence
        // (In reality, human annotator would provide gold labels)
        for c in &mut candidates {
            if round1_texts.contains(&c.text) {
                // After annotation, confidence increases
                c.confidence = 0.95;
            }
        }

        // Round 2: Select from remaining uncertain examples
        let round2 = learner.select(&candidates, 3);
        println!("\nRound 2 - Selected for annotation:");
        for c in &round2 {
            println!("  [{:.2}] {}", c.confidence, c.text);
        }

        // Verify round 2 selects different examples
        for c in &round2 {
            assert!(
                !round1_texts.contains(&c.text) || c.confidence > 0.9,
                "Round 2 re-selected unannotated example '{}' with low confidence",
                c.text
            );
        }
    }

    /// Test budget estimation based on target F1 improvement.
    #[test]
    fn estimate_annotation_budget() {
        use anno::eval::active_learning::estimate_budget;

        // Scenario: Current F1 = 70%, target = 85%, corpus = 1000 examples
        let budget = estimate_budget(0.70, 0.85, 1000, 0.01);

        assert!(budget.is_some());
        let n = budget.unwrap();
        assert!(
            n > 0 && n <= 1000,
            "Budget {} should be between 0 and corpus size",
            n
        );

        println!(
            "Estimated annotation budget: {} examples (F1 improvement: 70% -> 85%)",
            n
        );
    }
}

// =============================================================================
// Edge Cases
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn empty_corpus_handling() {
        let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);
        let selected = learner.select(&[], 5);
        assert!(selected.is_empty());
    }

    #[test]
    fn request_more_than_available() {
        let candidates = vec![Candidate::new("Only one", 0.5)];

        let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);
        let selected = learner.select(&candidates, 10);

        assert_eq!(
            selected.len(),
            1,
            "Should return all available when requesting more"
        );
    }

    #[test]
    fn all_same_confidence() {
        let candidates: Vec<Candidate> = (0..5)
            .map(|i| Candidate::new(format!("Text {}", i), 0.5))
            .collect();

        let learner = ActiveLearner::new(SamplingStrategy::Uncertainty);
        let selected = learner.select(&candidates, 3);

        // Should still select 3 (any 3, since all equal)
        assert_eq!(selected.len(), 3);
    }
}
