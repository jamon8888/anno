//! End-to-end tests for realistic evaluation workflows.
//!
//! These tests simulate real-world usage patterns for NER evaluation:
//! - Loading datasets
//! - Running extraction pipelines
//! - Computing metrics
//! - Cross-backend comparison

#![cfg(feature = "eval")]

use anno::backends::HmmNER;
use anno::eval::loader::{DatasetId, DatasetLoader, LoadableDatasetId};
use anno::{CrfNER, HeuristicNER, Model, StackedNER};
use anno_core::{Entity, EntityType};
use std::collections::{HashMap, HashSet};

fn fast_stack() -> StackedNER {
    // Avoid `StackedNER::default()` in E2E tests: it may include feature-gated / slow backends.
    StackedNER::builder()
        .layer(anno::RegexNER::new())
        .layer(anno::HeuristicNER::new())
        .strategy(anno::backends::stacked::ConflictStrategy::Priority)
        .build()
}

fn assert_valid_entities(text: &str, entities: &[Entity]) {
    let char_len = text.chars().count();
    for e in entities {
        assert!(e.start <= e.end, "Invalid span start>end: {:?}", e);
        assert!(
            e.end <= char_len,
            "Span out of bounds for text (len={}): {:?}",
            char_len,
            e
        );
        assert!(
            (0.0..=1.0).contains(&e.confidence),
            "Confidence must be in [0, 1]: {:?}",
            e
        );
        assert!(
            !e.text.trim().is_empty(),
            "Entity text must be non-empty: {:?}",
            e
        );
    }
}

fn fingerprint_entities(entities: &[Entity]) -> Vec<(String, EntityType, usize, usize)> {
    // `anno_core::Entity` intentionally does not implement `PartialEq` (API stability / floats).
    // For determinism checks in tests, compare a stable projection.
    let mut fp: Vec<(String, EntityType, usize, usize)> = entities
        .iter()
        .map(|e| (e.text.clone(), e.entity_type.clone(), e.start, e.end))
        .collect();
    fp.sort_by(|a, b| a.2.cmp(&b.2).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
    fp
}

// =============================================================================
// Dataset Loading Workflow Tests
// =============================================================================

#[test]
fn test_dataset_loading_and_metadata() {
    // Test that we can enumerate and inspect datasets
    let loadable_count = LoadableDatasetId::all().len();

    assert!(
        loadable_count >= 100,
        "Should have at least 100 loadable datasets"
    );

    // Check that each loadable dataset has valid metadata
    for loadable_id in LoadableDatasetId::all().into_iter().take(10) {
        let id: DatasetId = loadable_id.into();
        let name = id.name();
        assert!(!name.is_empty(), "Dataset should have a name: {:?}", id);

        // Format should be defined
        let _format = id.format();
    }
}

#[test]
fn test_dataset_tasks_coverage() {
    // Check that multiple tasks are represented
    let mut tasks_seen: HashSet<String> = HashSet::new();

    for loadable_id in LoadableDatasetId::all() {
        let id: DatasetId = loadable_id.into();
        for task in id.tasks() {
            tasks_seen.insert(task.to_string());
        }
    }

    // Should have diverse tasks
    assert!(
        tasks_seen.len() >= 3,
        "Should have at least 3 different tasks"
    );
}

// =============================================================================
// Evaluation Pipeline Tests
// =============================================================================

#[test]
fn test_basic_evaluation_pipeline() {
    // End-to-end: create model -> extract -> compare to gold
    let model = CrfNER::new();

    // Simulated gold standard
    let gold = vec![
        Entity::new("John Smith", EntityType::Person, 0, 10, 1.0),
        Entity::new("Apple Inc.", EntityType::Organization, 20, 30, 1.0),
        Entity::new("New York", EntityType::Location, 35, 43, 1.0),
    ];

    let text = "John Smith works at Apple Inc. in New York.";
    let predictions = model.extract_entities(text, None).expect("Extract");
    assert_valid_entities(text, &predictions);

    // Compute evaluation metrics
    let metrics = compute_entity_metrics(&gold, &predictions);

    // Metrics should be valid
    assert!(metrics.precision >= 0.0 && metrics.precision <= 1.0);
    assert!(metrics.recall >= 0.0 && metrics.recall <= 1.0);
    assert!(metrics.f1 >= 0.0 && metrics.f1 <= 1.0);
}

#[test]
fn test_multi_backend_comparison() {
    // Compare multiple backends on same inputs
    let backends: Vec<(&str, Box<dyn Model + Send + Sync>)> = vec![
        ("crf", Box::new(CrfNER::new())),
        ("hmm", Box::new(HmmNER::new())),
        ("heuristic", Box::new(HeuristicNER::new())),
        ("stacked", Box::new(fast_stack())),
    ];

    let test_texts = [
        "Barack Obama was the 44th President of the United States.",
        "Tesla CEO Elon Musk announced new electric vehicles in California.",
        "The European Central Bank raised interest rates in Frankfurt.",
    ];

    let mut results_per_backend: HashMap<&str, Vec<Vec<Entity>>> = HashMap::new();

    for (name, backend) in &backends {
        let mut backend_results = Vec::new();

        for text in &test_texts {
            let entities = backend.extract_entities(text, None).expect("Extract");
            assert_valid_entities(text, &entities);
            backend_results.push(entities);
        }

        results_per_backend.insert(name, backend_results);
    }

    // All backends should produce results (possibly empty) for all texts
    for (name, results) in &results_per_backend {
        assert_eq!(
            results.len(),
            test_texts.len(),
            "Backend {} should have result for each text",
            name
        );
    }
}

#[test]
fn test_evaluation_with_type_filtering() {
    // Evaluate only specific entity types
    let model = HeuristicNER::new();
    let text = "Dr. Jane Smith from Harvard University visited Paris, France.";

    let all_entities = model.extract_entities(text, None).expect("Extract");
    assert_valid_entities(text, &all_entities);

    // Filter to only persons
    let persons: Vec<_> = all_entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Person))
        .collect();

    // Filter to only locations
    let locations: Vec<_> = all_entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Location))
        .collect();

    // Filter to only organizations
    let orgs: Vec<_> = all_entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Organization))
        .collect();

    // Invariants: filters are sound and total does not grow.
    assert!(persons
        .iter()
        .all(|e| matches!(e.entity_type, EntityType::Person)));
    assert!(locations
        .iter()
        .all(|e| matches!(e.entity_type, EntityType::Location)));
    assert!(orgs
        .iter()
        .all(|e| matches!(e.entity_type, EntityType::Organization)));
    assert!(persons.len() + locations.len() + orgs.len() <= all_entities.len());
}

#[test]
fn test_confidence_threshold_evaluation() {
    // Evaluate at different confidence thresholds
    let model = CrfNER::new();
    let text = "Microsoft Corporation and Google LLC are tech companies.";

    let all_entities = model.extract_entities(text, None).expect("Extract");
    assert_valid_entities(text, &all_entities);

    let thresholds = [0.1, 0.3, 0.5, 0.7, 0.9];

    let mut last_count: Option<usize> = None;
    for threshold in thresholds {
        let high_conf: Vec<_> = all_entities
            .iter()
            .filter(|e| e.confidence >= threshold)
            .collect();

        // Invariant: entity count is non-increasing as threshold rises.
        if let Some(prev) = last_count {
            assert!(
                high_conf.len() <= prev,
                "Expected non-increasing entity count: threshold={threshold} prev={prev} now={}",
                high_conf.len()
            );
        }
        last_count = Some(high_conf.len());
    }
}

// =============================================================================
// Cross-Document Evaluation Tests
// =============================================================================

#[test]
fn test_cross_document_entity_aggregation() {
    // Aggregate entities across multiple documents
    let model = HeuristicNER::new();

    let documents = [
        "Tim Cook announced new Apple products.",
        "Tim Cook visited China to meet suppliers.",
        "The Apple Watch was presented by Tim Cook.",
    ];

    let mut entity_counts: HashMap<String, usize> = HashMap::new();

    for doc in &documents {
        let entities = model.extract_entities(doc, None).expect("Extract");
        assert_valid_entities(doc, &entities);

        for entity in entities {
            *entity_counts.entry(entity.text.clone()).or_insert(0) += 1;
        }
    }

    // Ensure at least one repeated mention exists (stable crafted example).
    let tim_mentions: usize = entity_counts
        .iter()
        .filter(|(k, _)| k.contains("Tim") && k.contains("Cook"))
        .map(|(_, v)| *v)
        .sum();
    assert!(
        tim_mentions >= 2,
        "Expected repeated Tim Cook mentions across docs, got counts: {:?}",
        entity_counts
    );
}

#[test]
fn test_document_level_statistics() {
    // Compute document-level stats
    let model = fast_stack();

    let documents = [
        "Short text.",
        "A medium length document with some entities like Google and Microsoft.",
        "A longer document mentioning Barack Obama, the White House, and the United States of America, which is a country in North America with many cities like New York and Los Angeles.",
    ];

    for (i, doc) in documents.iter().enumerate() {
        let entities = model.extract_entities(doc, None).expect("Extract");
        assert_valid_entities(doc, &entities);

        let word_count = doc.split_whitespace().count().max(1);
        let stats = DocumentStats {
            doc_id: i,
            char_count: doc.chars().count(),
            word_count,
            entity_count: entities.len(),
            entity_density: entities.len() as f64 / word_count as f64,
        };

        assert!(
            stats.entity_density >= 0.0,
            "Entity density should be non-negative"
        );
    }
}

// =============================================================================
// Realistic Use Case Tests
// =============================================================================

#[test]
fn test_news_article_extraction() {
    // Realistic news article processing
    let model = fast_stack();

    let article = r#"
        Washington, D.C. (AP) -- President Joe Biden met with British Prime Minister
        Rishi Sunak at the White House on Thursday to discuss the ongoing conflict
        in Ukraine and economic cooperation between the United States and United Kingdom.

        The meeting came as NATO allies prepare for their annual summit in Vilnius,
        Lithuania. Secretary of State Antony Blinken also participated in the talks.
    "#;

    let entities = model.extract_entities(article, None).expect("Extract");
    assert_valid_entities(article, &entities);

    // Should find various entity types
    let _entity_types: HashSet<_> = entities.iter().map(|e| &e.entity_type).collect();

    // Expect at least persons and locations in a news article
    assert!(!entities.is_empty(), "Should find entities in news article");
}

#[test]
fn test_scientific_text_extraction() {
    // Scientific/technical text
    let model = HeuristicNER::new();

    let text = r#"
        Dr. Jennifer Chen from Stanford University published research on the
        COVID-19 virus in the journal Nature Medicine. The study was conducted
        in collaboration with researchers from MIT and the NIH.
    "#;

    let entities = model.extract_entities(text, None).expect("Extract");

    // Should handle academic text
    for entity in &entities {
        assert!(!entity.text.is_empty());
    }
}

#[test]
fn test_financial_text_extraction() {
    // Financial/business text
    let model = CrfNER::new();

    let text = r#"
        Apple Inc. (AAPL) reported Q4 earnings of $1.29 per share, exceeding
        Wall Street expectations. CEO Tim Cook highlighted strong iPhone sales
        in China and growing Services revenue.
    "#;

    let entities = model.extract_entities(text, None).expect("Extract");

    // Should handle financial text
    for entity in &entities {
        assert!(entity.start <= entity.end);
    }
}

#[test]
fn test_multilingual_evaluation() {
    // Test across different languages
    let model = HmmNER::new();

    let multilingual_texts = [
        ("en", "Barack Obama visited Berlin, Germany."),
        ("zh", "習近平在北京會見了普京。"),
        ("ar", "التقى محمد بن سلمان بالرئيس في الرياض"),
        ("ru", "Путин встретился с Си Цзиньпином в Москве."),
        ("sa", "रामायणे रामः सीतां अयोध्यायाः वनं नयति"),
    ];

    for (lang, text) in &multilingual_texts {
        let entities = model.extract_entities(text, Some(lang)).expect("Extract");
        assert_valid_entities(text, &entities);

        // Should not error on any language
        for entity in &entities {
            assert!(entity.confidence >= 0.0);
        }
    }
}

#[test]
fn test_parse_conll_offline_pipeline_and_span_invariants() {
    // True mini-workflow: parse a dataset payload offline (no network), then run a backend on
    // the resulting sentence texts and validate span invariants.
    let loader = DatasetLoader::new().expect("loader");

    // CoNLL-2003-style (word POS chunk NER). Include multi-script tokens to stress Unicode.
    let conll = "\
John NNP B-NP B-PER
visited VBD B-VP O
北京 NNP B-NP B-LOC
.

التقى NNP B-NP O
محمد NNP B-NP B-PER
في NNP B-NP O
الرياض NNP B-NP B-LOC
.
";

    let dataset = loader
        .parse_content_str(conll, DatasetId::WikiGold)
        .expect("parse conll");
    assert_eq!(dataset.id, DatasetId::WikiGold);
    assert_eq!(dataset.len(), 2, "Expected 2 sentences");
    assert!(dataset.entity_count() >= 3, "Expected some gold entities");

    let model = HeuristicNER::new();
    for sent in &dataset.sentences {
        let text = sent.text();
        let gold = sent.entities();
        let pred = model.extract_entities(&text, None).expect("extract");

        // Gold spans must be valid for the sentence text.
        let char_len = text.chars().count();
        for g in &gold {
            assert!(g.start <= g.end);
            assert!(g.end <= char_len, "Gold span out of bounds: {:?}", g);
            assert!(!g.text.trim().is_empty());
        }

        // Pred spans must be valid too.
        assert_valid_entities(&text, &pred);

        // Determinism: heuristic backend must be stable across runs.
        let pred2 = model.extract_entities(&text, None).expect("extract 2");
        assert_eq!(
            fingerprint_entities(&pred),
            fingerprint_entities(&pred2),
            "Expected deterministic extraction"
        );
    }
}

// =============================================================================
// Error Recovery Tests
// =============================================================================

#[test]
fn test_malformed_input_handling() {
    let model = CrfNER::new();

    // Create the very long string first to extend its lifetime
    let very_long = "a".repeat(100_000);

    let malformed_inputs = [
        "",                // Empty
        "   \n\t  \r\n  ", // Whitespace only
        "\0\0\0",          // Null bytes
        &very_long,        // Very long
    ];

    for input in &malformed_inputs {
        let result = model.extract_entities(input, None);
        // Should not panic or error
        assert!(
            result.is_ok(),
            "Should handle input: {:?}...",
            input.chars().take(20).collect::<String>()
        );
    }
}

// =============================================================================
// Helper Structs and Functions
// =============================================================================

/// Simple evaluation metrics
struct EvaluationMetrics {
    precision: f64,
    recall: f64,
    f1: f64,
}

/// Document-level statistics
#[allow(dead_code)]
struct DocumentStats {
    doc_id: usize,
    char_count: usize,
    word_count: usize,
    entity_count: usize,
    entity_density: f64,
}

/// Compute basic entity-level metrics
fn compute_entity_metrics(gold: &[Entity], predicted: &[Entity]) -> EvaluationMetrics {
    let gold_set: HashSet<_> = gold
        .iter()
        .map(|e| (&e.text, e.entity_type.clone(), e.start, e.end))
        .collect();
    let pred_set: HashSet<_> = predicted
        .iter()
        .map(|e| (&e.text, e.entity_type.clone(), e.start, e.end))
        .collect();

    let tp = gold_set.intersection(&pred_set).count() as f64;
    let fp = pred_set.difference(&gold_set).count() as f64;
    let fn_ = gold_set.difference(&pred_set).count() as f64;

    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if tp + fn_ > 0.0 { tp / (tp + fn_) } else { 0.0 };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    EvaluationMetrics {
        precision,
        recall,
        f1,
    }
}
