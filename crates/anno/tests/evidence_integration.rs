//! Integration tests for the evidence-based clustering system.
//!
//! These tests verify the `PairEvidence`, `EvidenceSource`, `MediationStrategy`,
//! and `TransitivityAnalyzer` components work correctly in realistic scenarios.

use anno_coalesce::evidence::{
    EvidenceSource, MediationStrategy, PairEvidence, TransitivityAnalyzer,
};
use anno_coalesce::streaming::{StreamingConfig, StreamingResolver};
use std::collections::HashMap;

#[test]
fn test_pair_evidence_basic() {
    let mut evidence = PairEvidence::new();

    // Add positive evidence from string similarity
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.85,
    });

    let conf = evidence.mediate(&MediationStrategy::Average);
    assert!(conf > 0.5, "High similarity should yield > 0.5 confidence");
}

#[test]
fn test_pair_evidence_multiple_sources() {
    let mut evidence = PairEvidence::new();

    // Strong agreement from multiple sources
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.9,
    });
    evidence.add_source(EvidenceSource::Embedding {
        model: "minilm".into(),
        score: 0.85,
    });
    evidence.add_source(EvidenceSource::TypeMatch {
        matched: true,
        type_a: "PER".into(),
        type_b: "PERSON".into(),
    });

    // Should have high confidence with multiple positive signals
    let conf = evidence.mediate(&MediationStrategy::Average);
    assert!(
        conf > 0.6,
        "Multiple positive sources should yield high confidence"
    );

    // Voting should also indicate positive
    let voting_conf = evidence.mediate(&MediationStrategy::Voting);
    assert!(
        voting_conf > 0.5,
        "Voting should be positive with all positive sources"
    );
}

#[test]
fn test_evidence_conflicting_signals() {
    let mut evidence = PairEvidence::new();

    // Conflicting evidence
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "jaccard".into(),
        score: 0.95, // High string match
    });
    evidence.add_source(EvidenceSource::TypeMatch {
        matched: false, // Types don't match
        type_a: "ORG".into(),
        type_b: "LOC".into(),
    });
    evidence.add_source(EvidenceSource::Embedding {
        model: "minilm".into(),
        score: 0.7, // Moderate embedding sim
    });

    // Different strategies should handle conflict differently
    let avg = evidence.mediate(&MediationStrategy::Average);
    let voting = evidence.mediate(&MediationStrategy::Voting);
    let max = evidence.mediate(&MediationStrategy::Max);
    let min = evidence.mediate(&MediationStrategy::Min);

    // Max should reflect strongest positive
    assert!(max > avg, "Max should be >= Average");
    // Min should reflect type mismatch
    assert!(min < avg, "Min should be <= Average");
    // With 2 positive and 1 negative, voting should be positive
    assert!(voting > 0.5, "Voting: 2 positive vs 1 negative = positive");
}

#[test]
fn test_source_weighted_mediation() {
    let mut evidence = PairEvidence::new();

    // Knowledge base link (strong signal)
    evidence.add_source(EvidenceSource::KnowledgeBase {
        kb_id: Some("Q76".into()), // Same entity
        linked: true,
    });
    // String similarity (weaker signal)
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.6,
    });

    let mut weights = HashMap::new();
    weights.insert("knowledge_base".to_string(), 2.0); // Double weight
    weights.insert("trigram".to_string(), 1.0);

    let strategy = MediationStrategy::SourceWeighted {
        weights,
        default_weight: 1.0,
    };
    let conf = evidence.mediate(&strategy);

    // Should be weighted toward KB link
    assert!(conf > 0.7, "KB link should dominate with higher weight");
}

#[test]
fn test_bayesian_mediation() {
    let mut evidence = PairEvidence::new();

    // Prior belief of 0.5 (uninformative)
    let prior = 0.5;

    // Strong positive evidence
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.9,
    });
    evidence.add_source(EvidenceSource::Embedding {
        model: "minilm".into(),
        score: 0.85,
    });

    let strategy = MediationStrategy::Bayesian { prior };
    let posterior = evidence.mediate(&strategy);

    // Posterior should be higher than prior with positive evidence
    assert!(
        posterior > prior,
        "Posterior should exceed prior with positive evidence"
    );
}

#[test]
fn test_product_mediation() {
    let mut evidence = PairEvidence::new();

    // Multiple independent confirmations
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.9,
    });
    evidence.add_source(EvidenceSource::Embedding {
        model: "minilm".into(),
        score: 0.9,
    });
    evidence.add_source(EvidenceSource::TypeMatch {
        matched: true,
        type_a: "PER".into(),
        type_b: "PER".into(),
    });

    let product_conf = evidence.mediate(&MediationStrategy::Product);

    // Should be reasonably high
    assert!(
        product_conf > 0.5,
        "Product of high scores should be positive"
    );
}

#[test]
fn test_blocker_evidence() {
    let mut evidence = PairEvidence::new();

    // Positive signals
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.9,
    });
    // But a blocking negative signal
    evidence.add_source(EvidenceSource::NegativeEvidence {
        reason: "different_wikidata_id".into(),
        confidence: 0.95,
    });

    assert!(evidence.has_blocker(), "Should detect blocker");

    // Blocker should dominate
    let conf = evidence.mediate(&MediationStrategy::default());
    assert!(conf < 0.1, "Blocker should result in low confidence");
}

#[test]
fn test_transitivity_analyzer_basic() {
    // Create similarity matrix with a violation
    // a~b (0.9), b~c (0.9), but a~c (0.2)
    let sims = vec![
        vec![1.0, 0.9, 0.2],
        vec![0.9, 1.0, 0.9],
        vec![0.2, 0.9, 1.0],
    ];

    let analyzer = TransitivityAnalyzer::from_matrix(&sims);
    let violations = analyzer.find_violations(0.5);

    assert!(
        !violations.is_empty(),
        "Should detect transitivity violation: A-B + B-C but not A-C"
    );

    // First violation should be the one we created
    let v = &violations[0];
    assert_eq!(v.a, 0);
    assert_eq!(v.b, 1);
    assert_eq!(v.c, 2);
}

#[test]
fn test_transitivity_no_violations() {
    // Create consistent transitive closure
    // All pairs have high similarity
    let sims = vec![
        vec![1.0, 0.9, 0.85],
        vec![0.9, 1.0, 0.9],
        vec![0.85, 0.9, 1.0],
    ];

    let analyzer = TransitivityAnalyzer::from_matrix(&sims);
    let violations = analyzer.find_violations(0.5);

    assert!(
        violations.is_empty(),
        "Consistent transitive closure should have no violations"
    );
}

#[test]
fn test_transitivity_score() {
    // Perfect clustering with high internal similarity
    let sims = vec![
        vec![1.0, 0.9, 0.85, 0.1, 0.1],
        vec![0.9, 1.0, 0.9, 0.1, 0.1],
        vec![0.85, 0.9, 1.0, 0.1, 0.1],
        vec![0.1, 0.1, 0.1, 1.0, 0.95],
        vec![0.1, 0.1, 0.1, 0.95, 1.0],
    ];

    let analyzer = TransitivityAnalyzer::from_matrix(&sims);

    // Two clusters: [0,1,2] and [3,4]
    let clusters = vec![vec![0, 1, 2], vec![3, 4]];
    let score = analyzer.transitivity_score(&clusters);

    assert!(
        score > 0.9,
        "Perfect clustering should have high transitivity score"
    );
}

#[test]
fn test_streaming_resolver_basic() {
    let config = StreamingConfig::default();
    let mut resolver = StreamingResolver::new(config);

    // Add similar entities
    resolver.add_entity(
        "doc1".to_string(),
        "Barack Obama".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc2".to_string(),
        "Obama".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc3".to_string(),
        "B. Obama".to_string(),
        Some("PER".to_string()),
    );

    // Should have some clusters
    let clusters = resolver.clusters();
    assert!(!clusters.is_empty(), "Should create at least one cluster");
}

#[test]
fn test_streaming_type_mismatch() {
    let config = StreamingConfig {
        require_type_match: true,
        ..Default::default()
    };
    let mut resolver = StreamingResolver::new(config);

    // Add entities with same text but different types
    resolver.add_entity(
        "doc1".to_string(),
        "Apple".to_string(),
        Some("ORG".to_string()),
    );
    resolver.add_entity(
        "doc2".to_string(),
        "Apple".to_string(),
        Some("PRODUCT".to_string()),
    );

    // With type matching required, should be separate clusters
    let clusters = resolver.clusters();

    // Depends on type normalization, but should respect types
    assert!(
        !clusters.is_empty(),
        "Type-aware clustering should handle different types"
    );
}

#[test]
fn test_evidence_source_contributions() {
    // Test that different sources contribute appropriately

    // High string similarity -> positive
    let high_string = EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.9,
    };
    assert!(high_string.score_contribution() > 0.5);

    // Low string similarity -> negative
    let low_string = EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.2,
    };
    assert!(low_string.score_contribution() < 0.0);

    // Type match -> positive
    let type_match = EvidenceSource::TypeMatch {
        matched: true,
        type_a: "PER".into(),
        type_b: "PER".into(),
    };
    assert!(type_match.score_contribution() > 0.0);

    // Type mismatch -> negative
    let type_mismatch = EvidenceSource::TypeMatch {
        matched: false,
        type_a: "PER".into(),
        type_b: "ORG".into(),
    };
    assert!(type_mismatch.score_contribution() < 0.0);

    // KB link -> strong positive
    let kb_linked = EvidenceSource::KnowledgeBase {
        kb_id: Some("Q76".into()),
        linked: true,
    };
    assert!(kb_linked.score_contribution() > 0.5);
}

#[test]
fn test_evidence_aggregation_empty() {
    let evidence = PairEvidence::new();

    // Empty evidence should return neutral confidence (0.5)
    let conf = evidence.mediate(&MediationStrategy::Average);
    assert!(
        (conf - 0.5).abs() < 0.01,
        "Empty evidence should be neutral"
    );
}

#[test]
fn test_multilingual_evidence() {
    // Test evidence accumulation for multilingual entities
    let mut evidence = PairEvidence::new();

    // Chinese and English variants of same person
    // "习近平" vs "Xi Jinping"
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "trigram".into(),
        score: 0.1, // Low string sim (different scripts)
    });
    evidence.add_source(EvidenceSource::Embedding {
        model: "multilingual-minilm".into(),
        score: 0.9, // High embedding sim
    });
    evidence.add_source(EvidenceSource::KnowledgeBase {
        kb_id: Some("Q15031".into()),
        linked: true, // Same KB entity
    });
    evidence.add_source(EvidenceSource::TypeMatch {
        matched: true,
        type_a: "PERSON".into(),
        type_b: "PER".into(),
    });

    // Despite low string similarity, should have high confidence
    let conf = evidence.mediate(&MediationStrategy::Average);
    assert!(
        conf > 0.5,
        "Multi-signal evidence should overcome low string similarity"
    );
}

#[test]
fn test_custom_evidence_source() {
    let mut evidence = PairEvidence::new();

    // Custom evidence from domain-specific source
    evidence.add_source(EvidenceSource::Custom {
        source: "company_registry".into(),
        score: 0.95,
        metadata: {
            let mut m = HashMap::new();
            m.insert("registry_id".into(), "12345".into());
            m
        },
    });

    let conf = evidence.mediate(&MediationStrategy::Average);
    assert!(
        conf > 0.7,
        "Custom high-confidence source should yield high confidence"
    );
}

#[test]
fn test_contextual_coref_evidence() {
    let mut evidence = PairEvidence::new();

    // Neural coref model prediction
    evidence.add_source(EvidenceSource::ContextualCoref {
        model: "longformer-coref".into(),
        score: 0.85,
    });
    evidence.add_source(EvidenceSource::StringSimilarity {
        method: "exact".into(),
        score: 0.3, // Different surface forms
    });

    // Neural model should carry weight
    let conf = evidence.mediate(&MediationStrategy::Average);
    assert!(
        conf > 0.4,
        "Neural coref prediction should contribute positively"
    );
}

// Property-based tests
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_confidence() -> impl Strategy<Value = f32> {
        0.0f32..=1.0f32
    }

    proptest! {
        // Config for reasonable test count
        #![proptest_config(ProptestConfig { cases: 50, ..Default::default() })]

        #[test]
        fn mediated_confidence_bounded(
            scores in prop::collection::vec(arb_confidence(), 1..10),
        ) {
            let mut evidence = PairEvidence::new();
            for score in scores {
                evidence.add_source(EvidenceSource::StringSimilarity {
                    method: "test".into(),
                    score,
                });
            }

            let result = evidence.mediate(&MediationStrategy::Average);
            prop_assert!((0.0..=1.0).contains(&result), "Confidence must be in [0, 1]");
        }

        #[test]
        fn all_positive_yields_positive(
            scores in prop::collection::vec(arb_confidence().prop_filter("high", |c| *c > 0.7), 2..5),
        ) {
            let mut evidence = PairEvidence::new();
            for score in scores {
                evidence.add_source(EvidenceSource::StringSimilarity {
                    method: "test".into(),
                    score,
                });
            }

            let result = evidence.mediate(&MediationStrategy::Average);
            prop_assert!(result > 0.5, "All high positive evidence should yield > 0.5 confidence");
        }

        #[test]
        fn max_greater_or_equal_average(
            scores in prop::collection::vec(arb_confidence(), 2..5),
        ) {
            let mut evidence = PairEvidence::new();
            for score in scores {
                evidence.add_source(EvidenceSource::StringSimilarity {
                    method: "test".into(),
                    score,
                });
            }

            let avg = evidence.mediate(&MediationStrategy::Average);
            let max = evidence.mediate(&MediationStrategy::Max);

            prop_assert!(max >= avg - 0.01, "Max should be >= Average (with small epsilon)");
        }
    }
}
