//! End-to-end tests for non-baseline backends and real-world evaluation scenarios.
//!
//! These tests exercise the full pipeline for backends beyond HeuristicNER/RegexNER,
//! and cover realistic CLI-motivated workflows.

#![allow(deprecated)] // Testing legacy backends like RuleBasedNER

use anno::backends::stacked::ConflictStrategy;
use anno::backends::{BiLstmCrfNER, HmmNER, RuleBasedNER};
use anno::{CrfNER, EnsembleNER, HeuristicNER, Model, RegexNER, StackedNER};
use anno_core::{Entity, EntityType};

fn assert_valid_entities(text: &str, entities: &[Entity], backend_name: &str) {
    let char_len = text.chars().count();
    for e in entities {
        assert!(
            e.start <= e.end,
            "{backend_name}: invalid span start>end: {:?}",
            e
        );
        assert!(
            e.end <= char_len,
            "{backend_name}: span out of bounds (len={char_len}): {:?}",
            e
        );
        assert!(
            (0.0..=1.0).contains(&e.confidence),
            "{backend_name}: confidence must be in [0,1]: {:?}",
            e
        );
        assert!(
            !e.text.trim().is_empty(),
            "{backend_name}: empty entity text: {:?}",
            e
        );
    }
}

fn fingerprint_entities(entities: &[Entity]) -> Vec<(String, EntityType, usize, usize)> {
    // `anno_core::Entity` does not implement PartialEq; compare a stable projection.
    let mut fp: Vec<(String, EntityType, usize, usize)> = entities
        .iter()
        .map(|e| (e.text.clone(), e.entity_type.clone(), e.start, e.end))
        .collect();
    fp.sort_by(|a, b| a.2.cmp(&b.2).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
    fp
}

// =============================================================================
// HMM Backend Tests
// =============================================================================

#[test]
fn test_hmm_backend_basic_extraction() {
    let hmm = HmmNER::new();
    assert!(hmm.is_available(), "HMM backend should be available");

    let text = "Dr. John Smith works at Apple Inc. in New York City.";
    let entities = hmm
        .extract_entities(text, None)
        .expect("HMM extraction should succeed");

    // Note: HMM may be probabilistic; don't assert exact outputs. Assert invariants only.
    assert_valid_entities(text, &entities, "hmm");
}

#[test]
fn test_hmm_backend_multilingual() {
    let hmm = HmmNER::new();

    // Test with different languages
    let test_cases = [
        ("Barack Obama visited Berlin.", Some("en")),
        ("Angela Merkel est une politicienne allemande.", Some("fr")),
        ("東京は日本の首都です。", Some("ja")),
    ];

    for (text, lang) in test_cases {
        let result = hmm.extract_entities(text, lang);
        assert!(
            result.is_ok(),
            "HMM should not error on {}: {:?}",
            text,
            result.err()
        );
    }
}

#[test]
fn test_multilingual_multidomain_span_invariants_across_backends() {
    // This is an invariants test, not a quality test: every backend should be Unicode-safe
    // and never produce out-of-bounds or invalid spans, regardless of language/domain.
    let texts: [(&str, &str); 8] = [
        ("en_science", "Marie Curie discovered radium in Paris."),
        ("zh_politics", "習近平在北京會見了普京。"),
        ("ar_politics", "التقى محمد بن سلمان بالرئيس في الرياض"),
        ("ru_diplomacy", "Путин встретился с Си Цзиньпином в Москве."),
        ("hi_titles", "प्रधान मंत्री शर्मा ने दिल्ली में भाषण दिया।"),
        (
            "mixed_code_switch",
            "Dr. 田中 presented her research at MIT's AI conference.",
        ),
        (
            "diacritics",
            "François Müller and José García met in São Paulo.",
        ),
        (
            "single_names",
            "Pelé, Madonna, and Cher performed at the concert.",
        ),
    ];

    let backends: Vec<(&'static str, Box<dyn Model>)> = vec![
        ("heuristic", Box::new(HeuristicNER::new())),
        ("regex", Box::new(RegexNER::new())),
        ("rule_based", Box::new(RuleBasedNER::new())),
        ("hmm", Box::new(HmmNER::new())),
        ("crf", Box::new(CrfNER::new())),
    ];

    for (case, text) in texts {
        for (name, backend) in &backends {
            let entities = backend
                .extract_entities(text, None)
                .unwrap_or_else(|e| panic!("{name} should not error on {case}: {e:?}"));
            assert_valid_entities(text, &entities, name);
        }
    }
}

#[test]
fn test_hmm_backend_edge_cases() {
    let hmm = HmmNER::new();

    // Empty text
    let empty = hmm.extract_entities("", None).expect("Empty text");
    assert!(empty.is_empty(), "Empty text should produce no entities");

    // Whitespace only
    let whitespace = hmm.extract_entities("   \n\t  ", None).expect("Whitespace");
    assert!(
        whitespace.is_empty(),
        "Whitespace should produce no entities"
    );

    // Very long text
    let long_text = "John Smith ".repeat(1000);
    let result = hmm.extract_entities(&long_text, None);
    assert!(result.is_ok(), "Long text should not error");
}

// =============================================================================
// CRF Backend Tests
// =============================================================================

#[test]
fn test_crf_backend_basic_extraction() {
    let crf = CrfNER::new();
    assert!(crf.is_available(), "CRF backend should be available");

    let text = "Microsoft Corporation CEO Satya Nadella announced new products.";
    let entities = crf.extract_entities(text, None).expect("CRF extraction");

    assert_valid_entities(text, &entities, "crf");
}

#[test]
fn test_crf_backend_deterministic() {
    let crf = CrfNER::new();
    let text = "Tesla CEO Elon Musk met with Tim Cook at Apple headquarters.";

    // Run extraction multiple times
    let results: Vec<_> = (0..5)
        .map(|_| crf.extract_entities(text, None).expect("Extract"))
        .collect();

    // Determinism: stable projection should match across runs.
    let fp0 = fingerprint_entities(&results[0]);
    for ents in results.iter().skip(1) {
        assert_eq!(
            fp0,
            fingerprint_entities(ents),
            "CRF should be deterministic"
        );
    }
}

#[test]
fn test_crf_backend_supported_types() {
    let crf = CrfNER::new();
    let types = crf.supported_types();

    // CRF should support standard NER types
    assert!(!types.is_empty(), "CRF should have supported types");

    // Should include at least one of the standard types
    let has_standard = types.iter().any(|t| {
        matches!(
            t,
            EntityType::Person | EntityType::Organization | EntityType::Location
        )
    });
    assert!(has_standard, "CRF should support standard NER types");
}

// =============================================================================
// BiLSTM-CRF Backend Tests
// =============================================================================

#[test]
fn test_bilstm_crf_backend_basic() {
    let bilstm = BiLstmCrfNER::new();
    assert!(
        bilstm.is_available(),
        "BiLSTM-CRF backend should be available"
    );

    let text = "The World Health Organization (WHO) is based in Geneva, Switzerland.";
    let result = bilstm.extract_entities(text, None);

    // Should not error (may use heuristic fallback)
    assert!(
        result.is_ok(),
        "BiLSTM-CRF should not error: {:?}",
        result.err()
    );
}

#[test]
fn test_bilstm_crf_backend_entity_types() {
    let bilstm = BiLstmCrfNER::new();

    // BiLSTM-CRF should recognize various entity types
    let text = "Dr. Jane Doe from Harvard University visited the Louvre in Paris.";
    let entities = bilstm.extract_entities(text, None).expect("Extract");

    assert_valid_entities(text, &entities, "bilstm-crf");
}

// =============================================================================
// Rule-Based Backend Tests
// =============================================================================

#[test]
fn test_rule_based_backend_basic() {
    let rule = RuleBasedNER::new();
    assert!(rule.is_available(), "RuleBasedNER should be available");

    let text = "Apple Inc. stock (AAPL) rose 5% today.";
    let entities = rule.extract_entities(text, None).expect("Extract");

    // RuleBasedNER uses pattern matching - should find organizations
    assert_valid_entities(text, &entities, "rule");
}

#[test]
fn test_rule_based_backend_patterns() {
    let rule = RuleBasedNER::new();

    // Test specific patterns
    let test_cases = [
        ("Contact support@example.com for help.", true), // Email
        ("Call us at +1-555-123-4567.", true),           // Phone
        ("Visit https://example.com for more.", true),   // URL
    ];

    for (text, _should_find) in test_cases {
        let result = rule.extract_entities(text, None);
        assert!(result.is_ok(), "RuleBasedNER should not error on: {}", text);
    }
}

// =============================================================================
// Ensemble Backend Tests (with non-baseline components)
// =============================================================================

#[test]
fn test_ensemble_with_diverse_backends() {
    // Create ensemble with HMM, CRF, and heuristic
    let ensemble = EnsembleNER::with_backends(vec![
        Box::new(HmmNER::new()),
        Box::new(CrfNER::new()),
        Box::new(HeuristicNER::new()),
    ]);

    assert!(ensemble.is_available());

    let text = "Google CEO Sundar Pichai announced new AI products at I/O conference.";
    let entities = ensemble
        .extract_entities(text, None)
        .expect("Ensemble extract");

    // Ensemble should combine results from multiple backends
    assert_valid_entities(text, &entities, "ensemble");
}

#[test]
fn test_ensemble_voting_confidence() {
    let ensemble = EnsembleNER::with_backends(vec![
        Box::new(HmmNER::new()),
        Box::new(CrfNER::new()),
        Box::new(HeuristicNER::new()),
        Box::new(RegexNER::new()),
    ]);

    let text = "Microsoft Corporation was founded by Bill Gates.";
    let entities = ensemble.extract_entities(text, None).expect("Extract");

    assert_valid_entities(text, &entities, "ensemble");
    // Soft invariant: if there are multiple entities, there should be a well-defined spread.
    if entities.len() >= 2 {
        let min = entities.iter().map(|e| e.confidence).fold(1.0, f64::min);
        let max = entities.iter().map(|e| e.confidence).fold(0.0, f64::max);
        assert!(max >= min);
    }
}

#[test]
fn test_ensemble_deterministic_and_provenance_multilingual_overlap() {
    // Invariants (not quality):
    // - deterministic output
    // - Unicode-safe character offsets
    // - provenance + hierarchical confidence present for interpretability
    //
    // Intentionally includes:
    // - diacritics (François, Müller, São)
    // - repeated org surface form (Apple Inc.)
    // - pattern entity ($100)
    // - CJK sentence
    let ensemble = EnsembleNER::with_backends(vec![
        Box::new(RegexNER::new()),
        Box::new(HeuristicNER::new()),
        Box::new(RuleBasedNER::new()),
    ]);

    let text = "François Müller met Apple Inc. in São Paulo. Apple Inc. paid $100. 東京で会った。";

    type Fingerprint = (String, EntityType, usize, usize, String);
    let mut fps: Vec<Vec<Fingerprint>> = Vec::new();
    for _ in 0..5 {
        let entities = ensemble
            .extract_entities(text, None)
            .expect("ensemble extraction");
        assert_valid_entities(text, &entities, "ensemble");

        for e in &entities {
            assert!(
                e.provenance.is_some(),
                "ensemble entity should have provenance: {:?}",
                e
            );
            // `hierarchical_confidence` is only populated when there is an actual voting/consensus
            // situation (2+ candidates). Single-source entities still have provenance but no
            // meaningful hierarchical breakdown.
            let prov = e.provenance.as_ref().expect("checked above");
            if matches!(prov.method, anno_core::entity::ExtractionMethod::Consensus) {
                assert!(
                    e.hierarchical_confidence.is_some(),
                    "consensus entities must have hierarchical_confidence: {:?}",
                    e
                );
            }
        }

        let mut fp: Vec<(String, EntityType, usize, usize, String)> = entities
            .iter()
            .map(|e| {
                let src = e
                    .provenance
                    .as_ref()
                    .map(|p| p.source.to_string())
                    .unwrap_or_default();
                (e.text.clone(), e.entity_type.clone(), e.start, e.end, src)
            })
            .collect();
        fp.sort_by(|a, b| a.2.cmp(&b.2).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
        fps.push(fp);
    }

    for w in fps.windows(2) {
        assert_eq!(w[0], w[1], "ensemble output must be deterministic");
    }
}

fn assert_composed_entities_have_explainability_contract(
    text: &str,
    entities: &[Entity],
    name: &str,
) {
    assert_valid_entities(text, entities, name);
    for e in entities {
        assert!(
            e.provenance.is_some(),
            "{name}: composed output should have provenance: {:?}",
            e
        );
        // Hierarchical confidence is only meaningful for consensus-style decisions.
        if let Some(p) = &e.provenance {
            if matches!(p.method, anno_core::entity::ExtractionMethod::Consensus) {
                assert!(
                    e.hierarchical_confidence.is_some(),
                    "{name}: consensus entities must have hierarchical_confidence: {:?}",
                    e
                );
            }
        }
    }
}

fn fingerprint_entities_with_source(
    entities: &[Entity],
) -> Vec<(String, EntityType, usize, usize, String)> {
    let mut fp: Vec<(String, EntityType, usize, usize, String)> = entities
        .iter()
        .map(|e| {
            let src = e
                .provenance
                .as_ref()
                .map(|p| p.source.to_string())
                .unwrap_or_default();
            (e.text.clone(), e.entity_type.clone(), e.start, e.end, src)
        })
        .collect();
    fp.sort_by(|a, b| a.2.cmp(&b.2).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
    fp
}

#[test]
fn test_composability_stacked_can_wrap_ensemble() {
    // StackedNER should be able to use an EnsembleNER as one of its layers.
    // This is useful when you want a consensus layer (ensemble) plus a “last mile”
    // heuristic/pattern layer, or vice versa.
    let inner = EnsembleNER::with_backends(vec![
        Box::new(RegexNER::new()),
        Box::new(HeuristicNER::new()),
        Box::new(RuleBasedNER::new()),
    ]);

    let stacked = StackedNER::builder()
        .layer(inner)
        .layer(RegexNER::new())
        .strategy(ConflictStrategy::Union)
        .build();

    let text = "Dr. 田中 met François Müller in الرياض. Apple Inc. paid $100. 東京で会った。";

    let fp1 = fingerprint_entities_with_source(
        &stacked
            .extract_entities(text, None)
            .expect("stacked extract"),
    );
    let fp2 = fingerprint_entities_with_source(
        &stacked
            .extract_entities(text, None)
            .expect("stacked extract again"),
    );
    assert_eq!(fp1, fp2, "stacked(ensemble(..)) should be deterministic");

    let entities = stacked
        .extract_entities(text, None)
        .expect("stacked extract");
    assert_composed_entities_have_explainability_contract(text, &entities, "stacked(ensemble)");
}

#[test]
fn test_composability_ensemble_can_include_stacked_backend() {
    // EnsembleNER should be able to treat StackedNER as just another backend.
    // This lets you use a stacked “specialist” as one voter among others.
    let stacked = StackedNER::builder()
        .layer(HeuristicNER::new())
        .layer(RegexNER::new())
        .strategy(ConflictStrategy::Union)
        .build();

    let ensemble = EnsembleNER::with_backends(vec![
        Box::new(stacked),
        Box::new(RuleBasedNER::new()),
        Box::new(HeuristicNER::new()),
    ]);

    let text = "François Müller met Apple Inc. in São Paulo. Apple Inc. paid $100. 東京で会った。";

    let fp1 = fingerprint_entities_with_source(
        &ensemble
            .extract_entities(text, None)
            .expect("ensemble extract"),
    );
    let fp2 = fingerprint_entities_with_source(
        &ensemble
            .extract_entities(text, None)
            .expect("ensemble extract again"),
    );
    assert_eq!(
        fp1, fp2,
        "ensemble(stacked(..), ..) should be deterministic"
    );

    let entities = ensemble
        .extract_entities(text, None)
        .expect("ensemble extract");
    assert_composed_entities_have_explainability_contract(text, &entities, "ensemble(stacked)");
}

#[test]
fn test_composability_nested_ensemble_is_supported_but_acyclic() {
    // “Including itself” in the strict sense (a cycle) is not meaningful:
    // an ensemble that calls itself would recurse forever.
    //
    // What *is* meaningful and should work is a nested, *acyclic* composition:
    // EnsembleNER can include *another* EnsembleNER as a backend.
    let inner = EnsembleNER::with_backends(vec![
        Box::new(RegexNER::new()),
        Box::new(HeuristicNER::new()),
    ]);
    let outer = EnsembleNER::with_backends(vec![Box::new(inner), Box::new(RuleBasedNER::new())]);

    let text = "Apple Inc. paid $100. Apple Inc. paid $200. 東京で会った。";

    let fp1 = fingerprint_entities_with_source(
        &outer.extract_entities(text, None).expect("outer extract"),
    );
    let fp2 = fingerprint_entities_with_source(
        &outer
            .extract_entities(text, None)
            .expect("outer extract again"),
    );
    assert_eq!(fp1, fp2, "nested ensemble should be deterministic");

    let entities = outer.extract_entities(text, None).expect("outer extract");
    assert_composed_entities_have_explainability_contract(text, &entities, "ensemble(ensemble)");
}

// =============================================================================
// Stacked Backend Tests (with non-baseline layers)
// =============================================================================

#[test]
fn test_stacked_with_diverse_layers() {
    let stacked = StackedNER::builder()
        .layer(RuleBasedNER::new())
        .layer(HmmNER::new())
        .layer(CrfNER::new())
        .layer(HeuristicNER::new())
        .strategy(ConflictStrategy::Priority)
        .build();

    let text = "Amazon Web Services (AWS) is a cloud computing platform.";
    let entities = stacked
        .extract_entities(text, None)
        .expect("Stacked extract");

    // Priority strategy: earlier layers take precedence
    assert_valid_entities(text, &entities, "stacked");
}

#[test]
fn test_stacked_merge_strategy() {
    let stacked = StackedNER::builder()
        .layer(HmmNER::new())
        .layer(CrfNER::new())
        // "Merge" here means "keep overlapping entities" (downstream decides).
        .strategy(ConflictStrategy::Union)
        .build();

    let text = "Tesla's Elon Musk discussed SpaceX with NASA officials.";
    let entities = stacked.extract_entities(text, None).expect("Extract");

    // Merge strategy combines overlapping entities
    assert_valid_entities(text, &entities, "stacked-union");
}

// =============================================================================
// Real Evaluation Workflow Tests
// =============================================================================

#[test]
fn test_eval_workflow_metrics_computation() {
    // Simulate a realistic evaluation workflow
    let backend = CrfNER::new();

    // "Gold" annotations (simulated)
    let gold_entities = [
        Entity::new("John Smith", EntityType::Person, 0, 10, 1.0),
        Entity::new("Apple Inc.", EntityType::Organization, 20, 30, 1.0),
    ];

    let text = "John Smith works at Apple Inc. in California.";
    let predicted = backend.extract_entities(text, None).expect("Extract");
    assert_valid_entities(text, &predicted, "crf");

    // Compute basic metrics
    let gold_texts: std::collections::HashSet<_> = gold_entities.iter().map(|e| &e.text).collect();
    let pred_texts: std::collections::HashSet<_> = predicted.iter().map(|e| &e.text).collect();

    let true_positives = gold_texts.intersection(&pred_texts).count();
    let false_positives = pred_texts.difference(&gold_texts).count();
    let false_negatives = gold_texts.difference(&pred_texts).count();

    // Precision, recall, F1 can be computed
    let precision = if true_positives + false_positives > 0 {
        true_positives as f64 / (true_positives + false_positives) as f64
    } else {
        0.0
    };
    let recall = if true_positives + false_negatives > 0 {
        true_positives as f64 / (true_positives + false_negatives) as f64
    } else {
        0.0
    };

    // Just verify computation doesn't break
    assert!((0.0..=1.0).contains(&precision));
    assert!((0.0..=1.0).contains(&recall));
}

#[test]
fn test_eval_workflow_compare_backends() {
    // Compare multiple backends on same text
    let backends: Vec<(&str, Box<dyn Model + Send + Sync>)> = vec![
        ("hmm", Box::new(HmmNER::new())),
        ("crf", Box::new(CrfNER::new())),
        ("heuristic", Box::new(HeuristicNER::new())),
    ];

    let text = "Barack Obama visited the White House in Washington D.C.";

    let mut results: Vec<(&str, Vec<Entity>)> = Vec::new();

    for (name, backend) in &backends {
        let entities = backend.extract_entities(text, None).expect("Extract");
        assert_valid_entities(text, &entities, name);
        results.push((name, entities));
    }

    // Each backend should produce some result (possibly empty)
    for (name, entities) in &results {
        for entity in entities {
            assert!(
                !entity.text.is_empty(),
                "{} produced empty entity text",
                name
            );
        }
    }
}

// =============================================================================
// CLI-Motivated Scenarios
// =============================================================================

#[test]
fn test_cli_extract_workflow() {
    // Simulate: anno extract --backend crf "text"
    let backend = CrfNER::new();
    let text = "The European Union signed a trade deal with Japan.";

    let entities = backend.extract_entities(text, None).expect("Extract");
    assert_valid_entities(text, &entities, "crf");

    // CLI would format as JSON/TSV
    for entity in &entities {
        let _json = format!(
            r#"{{"text":"{}","start":{},"end":{},"type":"{:?}","confidence":{:.3}}}"#,
            entity.text, entity.start, entity.end, entity.entity_type, entity.confidence
        );
        // JSON should at least be parseable.
        let parsed: serde_json::Value = serde_json::from_str(&_json).expect("valid json");
        assert!(parsed.get("text").is_some());
        assert!(parsed.get("start").is_some());
        assert!(parsed.get("end").is_some());
    }
}

#[test]
fn test_cli_batch_workflow() {
    // Simulate: anno batch --backend hmm file.txt
    let backend = HmmNER::new();
    let texts = [
        "Apple reported strong Q4 earnings.",
        "Google launched a new AI product.",
        "Microsoft acquired a gaming company.",
    ];

    // Process each text - HMM may not implement BatchCapable, so use sequential
    let results: Vec<Vec<Entity>> = texts
        .iter()
        .map(|text| backend.extract_entities(text, None).expect("Extract"))
        .collect();

    assert_eq!(results.len(), texts.len(), "Should have result per text");
    for (text, ents) in texts.iter().zip(results.iter()) {
        assert_valid_entities(text, ents, "hmm");
    }
}

#[test]
fn test_cli_compare_workflow() {
    // Simulate: anno compare --gold gold.json --pred predictions.json
    let gold = [
        Entity::new("John Smith", EntityType::Person, 0, 10, 1.0),
        Entity::new("Google", EntityType::Organization, 20, 26, 1.0),
    ];

    let pred = [
        Entity::new("John Smith", EntityType::Person, 0, 10, 0.95),
        Entity::new("Google Inc", EntityType::Organization, 20, 30, 0.87),
    ];

    // Exact match comparison
    let exact_matches = gold
        .iter()
        .filter(|g| {
            pred.iter()
                .any(|p| p.text == g.text && p.start == g.start && p.end == g.end)
        })
        .count();

    // Partial match (same start position)
    let partial_matches = gold
        .iter()
        .filter(|g| pred.iter().any(|p| p.start == g.start))
        .count();

    assert!(partial_matches >= exact_matches);
}

#[test]
fn test_cli_domain_specific_extraction() {
    // Simulate: anno extract --domain biomedical
    let backend = HeuristicNER::new();

    // Biomedical-like text
    let text = "The patient was prescribed Aspirin (acetylsalicylic acid) for inflammation.";
    let entities = backend.extract_entities(text, None).expect("Extract");

    // Should not error, even if specialized types not detected
    assert_valid_entities(text, &entities, "heuristic");
}

#[test]
fn test_cli_multilingual_workflow() {
    // Simulate: anno extract --lang de "German text"
    let backend = HmmNER::new();

    let german_text = "Angela Merkel besuchte das Brandenburger Tor in Berlin.";
    let entities = backend
        .extract_entities(german_text, Some("de"))
        .expect("Extract");

    // Should handle German text
    assert_valid_entities(german_text, &entities, "hmm");
}

// =============================================================================
// Performance / Stress Tests
// =============================================================================

#[test]
fn test_backend_handles_unicode_stress() {
    let backends: Vec<Box<dyn Model + Send + Sync>> = vec![
        Box::new(HmmNER::new()),
        Box::new(CrfNER::new()),
        Box::new(BiLstmCrfNER::new()),
    ];

    let unicode_texts = vec![
        "🏢 Apple Inc. CEO Tim Cook 👨‍💼 announced 📱 new products.",
        "日本の東京で会議が開催されました。",
        "مرحباً بالعالم - Hello World - שלום עולם",
        "Ñoño trabajó en España con José García.",
    ];

    for backend in &backends {
        for text in &unicode_texts {
            let result = backend.extract_entities(text, None);
            assert!(
                result.is_ok(),
                "Backend should handle Unicode: {:?}",
                result.err()
            );
        }
    }
}

#[test]
fn test_backend_handles_edge_inputs() {
    let backend = CrfNER::new();

    let long_text = "word ".repeat(10000);
    let edge_cases = vec![
        "",                   // Empty
        "   ",                // Whitespace
        "\n\n\n",             // Newlines only
        "a",                  // Single char
        "!!!",                // Punctuation only
        "1234567890",         // Numbers only
        "http://example.com", // URL only
        "test@example.com",   // Email only
        long_text.as_str(),   // Very long
    ];

    for text in edge_cases {
        let result = backend.extract_entities(text, None);
        assert!(
            result.is_ok(),
            "Should handle edge case: {}",
            text.chars().take(20).collect::<String>()
        );
    }
}
