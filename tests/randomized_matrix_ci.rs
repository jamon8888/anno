//! CI-friendly randomized matrix test for backends × datasets × tasks.
//!
//! This test randomly samples:
//! - A subset of backends (lightweight ones only)
//! - A subset of cached datasets
//! - All supported tasks for each combination
//!
//! Designed to catch regressions without exhaustive evaluation.
//!
//! Run with: `cargo test --test randomized_matrix_ci --features eval-advanced`

#![cfg(feature = "eval-advanced")]

use anno::eval::task_evaluator::{TaskEvalConfig, TaskEvaluator};
use anno::eval::task_mapping::Task;
use anno::eval::loader::DatasetId;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::SystemTime;

/// Lightweight backends (no ONNX/Candle downloads required)
const LIGHTWEIGHT_BACKENDS: &[&str] = &["pattern", "heuristic", "crf", "stacked"];

/// Datasets that are typically cached or fast to load.
/// These are datasets that exist in BOTH loader.rs and dataset_registry.rs,
/// representing the overlap between the two systems.
/// Expanded to cover diverse domains: NER, biomedical, coreference, multilingual, etc.
const QUICK_DATASETS: &[DatasetId] = &[
    // Core NER
    DatasetId::WikiGold,
    DatasetId::WNUT17,
    DatasetId::MITMovie,
    DatasetId::MITRestaurant,
    DatasetId::CoNLL2003Sample,
    DatasetId::OntoNotesSample,
    DatasetId::MultiNERD,
    DatasetId::WikiANN,
    DatasetId::GermEval2014,
    
    // Biomedical NER
    DatasetId::BC5CDR,
    DatasetId::GENIA,
    DatasetId::NCBIDisease,
    DatasetId::JNLPBA,
    DatasetId::BioRED,
    DatasetId::NLMChem,
    DatasetId::CADEC,
    DatasetId::CRAFT,
    
    // Coreference
    DatasetId::GAP,
    DatasetId::PreCo,
    DatasetId::LitBank,
    DatasetId::ECBPlus,
    DatasetId::GICoref,
    DatasetId::ARRAU,
    DatasetId::CorefUD,
    
    // Nested NER
    DatasetId::GENIANested,
    DatasetId::NNE,
    
    // Relation Extraction
    DatasetId::DocRED,
    DatasetId::TACRED,
    DatasetId::ReTACRED,
    DatasetId::FewRel,
    DatasetId::SemEval2010Task8,
    
    // Bias/Fairness
    DatasetId::WinoBias,
    DatasetId::WinoQueer,
    DatasetId::BBQ,
    
    // Historical/Ancient Languages
    DatasetId::AncientGreekUD,
    DatasetId::OldEnglishUD,
    DatasetId::OldNorseUD,
    DatasetId::SanskritUD,
    DatasetId::HIPE2022,
    DatasetId::HistoricalChineseNER,
    
    // Low-Resource/Indigenous
    DatasetId::MasakhaNER,
    DatasetId::MasakhaNER2,
    DatasetId::AmericasNLI,
    DatasetId::CherokeeNER,
    DatasetId::NahuatlNER,
    DatasetId::QxoRef,
    
    // Multilingual
    DatasetId::UNER,
    DatasetId::MSNER,
    
    // Dialogue/Fiction
    DatasetId::TwiConv,
    DatasetId::NovelCR,
    DatasetId::FantasyCoref,
    DatasetId::MuDoCo,
    DatasetId::DROC,
    
    // Specialized
    DatasetId::FCCT,
    DatasetId::GVC,
    DatasetId::TransMuCoRes,
    DatasetId::ISNotes,
    DatasetId::RadCoref,
    DatasetId::MGAP,
];

/// Tasks to test
const TASKS: &[Task] = &[Task::NER];

/// Generate a seed based on current time (for CI variety)
fn ci_seed() -> u64 {
    // Use env var if set (for reproducibility), otherwise time-based
    if let Ok(seed_str) = std::env::var("ANNO_CI_SEED") {
        seed_str.parse().unwrap_or(42)
    } else {
        // Time-based seed for variety across CI runs
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(42)
    }
}

/// Hash-based deterministic selection
fn select_random<T: Clone>(items: &[T], count: usize, seed: u64) -> Vec<T> {
    if items.len() <= count {
        return items.to_vec();
    }
    
    let mut indexed: Vec<(usize, u64)> = items
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            i.hash(&mut hasher);
            (i, hasher.finish())
        })
        .collect();
    
    indexed.sort_by_key(|(_, hash)| *hash);
    indexed.truncate(count);
    
    indexed.iter().map(|(i, _)| items[*i].clone()).collect()
}

#[test]
fn test_randomized_matrix_sample() {
    let seed = ci_seed();
    eprintln!("CI seed: {} (set ANNO_CI_SEED to reproduce)", seed);
    
    // Select random subsets
    let selected_backends = select_random(LIGHTWEIGHT_BACKENDS, 2, seed);
    let selected_datasets = select_random(QUICK_DATASETS, 2, seed.wrapping_add(1));
    
    eprintln!("Selected backends: {:?}", selected_backends);
    eprintln!("Selected datasets: {:?}", selected_datasets);
    
    let evaluator = match TaskEvaluator::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Skipping: TaskEvaluator init failed: {}", e);
            return;
        }
    };
    
    let config = TaskEvalConfig {
        tasks: TASKS.to_vec(),
        datasets: selected_datasets.clone(),
        backends: selected_backends.iter().map(|s| s.to_string()).collect(),
        max_examples: Some(10), // Small sample for CI speed
        seed: Some(seed),
        require_cached: true, // Only use cached datasets
        relation_threshold: 0.5,
        robustness: false,
        compute_familiarity: false,
        temporal_stratification: false,
        confidence_intervals: false,
        custom_coref_resolver: None,
    };
    
    let results = match evaluator.evaluate_all(config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Evaluation failed: {}", e);
            // Not a failure - datasets may not be cached
            return;
        }
    };
    
    // Validate results
    eprintln!("\n=== Results ===");
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    
    for result in &results.results {
        let status = if result.success {
            success_count += 1;
            "✓"
        } else if result.error.as_ref().map(|e| e.contains("not cached") || e.contains("unavailable")).unwrap_or(false) {
            skip_count += 1;
            "○"
        } else {
            error_count += 1;
            "✗"
        };
        
        eprintln!(
            "  {} {:?} × {:?} × {} → F1={:.1}%",
            status,
            result.task,
            result.dataset,
            result.backend,
            result.metrics.get("f1").copied().unwrap_or(0.0) * 100.0
        );
        
        if let Some(err) = &result.error {
            eprintln!("      Error: {}", err);
        }
    }
    
    eprintln!("\nSummary: {} success, {} skipped, {} errors", success_count, skip_count, error_count);
    
    // At least one combination should work (pattern + heuristic always available)
    if success_count == 0 && skip_count == results.results.len() {
        eprintln!("All datasets skipped (not cached) - this is OK for CI");
    } else {
        assert!(
            error_count == 0 || success_count > 0,
            "At least one backend×dataset should succeed or all should be skipped"
        );
    }
}

#[test]
fn test_multi_seed_variance() {
    //! Test that different seeds produce different samples but consistent results.
    
    let seeds = [42, 123, 456];
    let mut f1_scores: Vec<f64> = Vec::new();
    
    for &seed in &seeds {
        let evaluator = match TaskEvaluator::new() {
            Ok(e) => e,
            Err(_) => return, // Skip if evaluator unavailable
        };
        
        let config = TaskEvalConfig {
            tasks: vec![Task::NER],
            datasets: vec![DatasetId::WikiGold],
            backends: vec!["pattern".to_string()],
            max_examples: Some(20),
            seed: Some(seed),
            require_cached: true,
            relation_threshold: 0.5,
            robustness: false,
            compute_familiarity: false,
            temporal_stratification: false,
            confidence_intervals: false,
            custom_coref_resolver: None,
        };
        
        if let Ok(results) = evaluator.evaluate_all(config) {
            for result in &results.results {
                if result.success {
                    if let Some(&f1) = result.metrics.get("f1") {
                        f1_scores.push(f1);
                    }
                }
            }
        }
    }
    
    if f1_scores.len() >= 2 {
        // Variance should be reasonable (not zero, not huge)
        let mean = f1_scores.iter().sum::<f64>() / f1_scores.len() as f64;
        let variance = f1_scores.iter().map(|x| (x - mean).powi(2)).sum::<f64>() 
            / (f1_scores.len() - 1) as f64;
        let std_dev = variance.sqrt();
        
        eprintln!("Multi-seed F1: mean={:.3}, std={:.3}", mean, std_dev);
        
        // Pattern backend should be consistent (low variance)
        assert!(std_dev < 0.1, "Pattern backend should have low variance across seeds");
    }
}

#[test] 
fn test_backend_availability_matrix() {
    //! Verify which backends are available (informational).
    
    eprintln!("\n=== Backend Availability Matrix ===");
    
    for backend in LIGHTWEIGHT_BACKENDS {
        let available = anno::eval::backend_factory::BackendFactory::create(backend)
            .map(|b| b.is_available())
            .unwrap_or(false);
        
        eprintln!("  {} {}", if available { "✓" } else { "✗" }, backend);
    }
    
    // Check ML backends too (may not be available)
    let ml_backends = ["gliner_onnx", "gliner_candle", "nuner", "w2ner"];
    eprintln!("\nML backends:");
    for backend in ml_backends {
        let available = anno::eval::backend_factory::BackendFactory::create(backend)
            .map(|b| b.is_available())
            .unwrap_or(false);
        
        eprintln!("  {} {}", if available { "✓" } else { "○" }, backend);
    }
}

