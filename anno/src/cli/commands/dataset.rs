//! Dataset command - Work with NER datasets
//!
//! Provides CLI access to the dataset registry and loader.
//! - `list`: Browse available datasets from the registry
//! - `info`: Show metadata and statistics for a dataset
//! - `eval`: Evaluate a model on a dataset

use clap::{Parser, Subcommand};
#[cfg(feature = "eval-advanced")]
use itertools::Itertools;
#[cfg(feature = "eval")]
use std::time::Instant;

use super::super::output::color;
use super::super::parser::{EvalTask, ModelBackend};

#[cfg(feature = "eval-advanced")]
use super::super::utils::create_entity_pair_relations;

#[cfg(feature = "eval")]
use super::super::utils::types_match_flexible;

#[cfg(feature = "eval")]
use crate::eval::loader::DatasetId;

/// Work with NER datasets
#[derive(Parser, Debug)]
pub struct DatasetArgs {
    /// Action to perform
    #[command(subcommand)]
    pub action: DatasetAction,
}

/// Dataset subcommand actions.
#[derive(Subcommand, Debug)]
pub enum DatasetAction {
    /// List available datasets
    #[command(visible_alias = "ls")]
    List {
        /// Filter by task (ner, coref, re, el)
        #[arg(short, long)]
        task: Option<String>,

        /// Filter by domain (biomedical, news, social_media, etc.)
        #[arg(short, long)]
        domain: Option<String>,

        /// Show only loadable datasets (vs full registry)
        #[arg(long)]
        loadable: bool,

        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show dataset statistics
    #[command(visible_alias = "i")]
    Info {
        /// Dataset name
        #[arg(short, long)]
        dataset: String,
    },

    /// Evaluate model on dataset
    #[command(visible_alias = "e")]
    Eval {
        /// Dataset name
        #[arg(short, long, default_value = "synthetic")]
        dataset: String,

        /// Model backend to use
        #[arg(short, long, default_value = "stacked")]
        model: ModelBackend,

        /// Task type: ner, coref, or relation
        #[arg(short, long, default_value = "ner")]
        task: EvalTask,
    },

    /// Export annotations to brat/CoNLL/JSONL formats
    #[command(visible_alias = "ex")]
    Export(super::export::ExportArgs),

    /// Import annotations from brat/CoNLL/JSONL formats
    #[command(visible_alias = "im")]
    Import(super::import::ImportArgs),

    /// View entities with surrounding context for review
    #[command(visible_alias = "ctx")]
    Context(super::context::ContextArgs),
}

/// Execute the dataset command.
pub fn run(args: DatasetArgs) -> Result<(), String> {
    match args.action {
        DatasetAction::List {
            task,
            domain,
            loadable,
            verbose,
        } => {
            run_list(task, domain, loadable, verbose)?;
        }
        DatasetAction::Info { dataset } => {
            run_info(&dataset)?;
        }
        DatasetAction::Eval {
            dataset,
            model,
            task,
        } => {
            #[cfg(feature = "eval")]
            {
                let m = model.create_model()?;

                let (name, test_cases) = if dataset == "synthetic" {
                    (
                        "synthetic".to_string(),
                        vec![
                            (
                                "Marie Curie won the Nobel Prize.".to_string(),
                                vec![
                                    crate::eval::GoldEntity {
                                        text: "Marie Curie".to_string(),
                                        original_label: "PER".to_string(),
                                        entity_type: anno_core::EntityType::Person,
                                        start: 0,
                                        end: 11,
                                    },
                                    crate::eval::GoldEntity {
                                        text: "Nobel Prize".to_string(),
                                        original_label: "MISC".to_string(),
                                        entity_type: anno_core::EntityType::Other(
                                            "MISC".to_string(),
                                        ),
                                        start: 20,
                                        end: 31,
                                    },
                                ],
                            ),
                            (
                                "Apple Inc. is based in California.".to_string(),
                                vec![
                                    crate::eval::GoldEntity {
                                        text: "Apple Inc.".to_string(),
                                        original_label: "ORG".to_string(),
                                        entity_type: anno_core::EntityType::Organization,
                                        start: 0,
                                        end: 10,
                                    },
                                    crate::eval::GoldEntity {
                                        text: "California".to_string(),
                                        original_label: "LOC".to_string(),
                                        entity_type: anno_core::EntityType::Location,
                                        start: 24,
                                        end: 34,
                                    },
                                ],
                            ),
                            (
                                "Contact john@example.com for help.".to_string(),
                                vec![crate::eval::GoldEntity {
                                    text: "john@example.com".to_string(),
                                    original_label: "EMAIL".to_string(),
                                    entity_type: anno_core::EntityType::Other("EMAIL".to_string()),
                                    start: 8,
                                    end: 24,
                                }],
                            ),
                        ],
                    )
                } else {
                    // Parse dataset ID
                    let dataset_id: DatasetId = dataset
                        .parse::<DatasetId>()
                        .map_err(|e| format!("Invalid dataset '{}': {}", dataset, e))?;

                    #[cfg(not(feature = "eval-advanced"))]
                    {
                        let _ = dataset_id; // Suppress unused warning
                        return Err(
                            "Loading real datasets requires --features eval-advanced".to_string()
                        );
                    }

                    #[cfg(feature = "eval-advanced")]
                    {
                        use crate::eval::loader::DatasetLoader;

                        let loader = DatasetLoader::new()
                            .map_err(|e| format!("Failed to init dataset loader: {}", e))?;

                        println!(
                            "Loading {} (may download if not cached)...",
                            dataset_id.name()
                        );
                        let ds = loader
                            .load_or_download(dataset_id)
                            .map_err(|e| format!("Failed to load dataset: {}", e))?;

                        // Only warn if evaluating NER on non-NER dataset (not for coref/relation tasks)
                        if matches!(task, EvalTask::Ner)
                            && (dataset_id.is_coreference() || dataset_id.is_relation_extraction())
                        {
                            println!("{} Warning: Evaluating NER on non-NER dataset. Results may be empty.", color("33", "!"));
                        }

                        (ds.stats().name, ds.to_test_cases())
                    }
                };

                // Parse dataset ID once for reuse (avoid duplicate parsing)
                // Store Result to preserve error message if parsing fails
                #[cfg(feature = "eval-advanced")]
                let parsed_dataset_result: Result<DatasetId, String> = if dataset != "synthetic" {
                    dataset
                        .parse::<DatasetId>()
                        .map_err(|e| format!("Invalid dataset '{}': {}", dataset, e))
                } else {
                    Err("synthetic dataset".to_string()) // Not a real error, just indicates synthetic
                };

                // Route to appropriate evaluation based on task
                match task {
                    EvalTask::Ner => {
                        #[cfg(feature = "eval-advanced")]
                        let type_mapper: Option<crate::TypeMapper> = parsed_dataset_result
                            .as_ref()
                            .ok()
                            .and_then(|id| id.type_mapper());
                        #[cfg(not(feature = "eval-advanced"))]
                        let type_mapper: Option<crate::TypeMapper> = None;

                        println!();
                        println!("Evaluating {} on {} dataset (NER)...", model.name(), name);
                        if type_mapper.is_some() {
                            println!(
                                "  {} Using type mapping for domain-specific dataset",
                                color("33", "!")
                            );
                        }
                        println!("  Sentences: {}", test_cases.len());
                        println!();

                        // Per-entity-type tracking: (gold, pred, correct)
                        let mut per_type_stats: std::collections::HashMap<
                            String,
                            (usize, usize, usize),
                        > = std::collections::HashMap::new();
                        let mut total_gold = 0;
                        let mut total_pred = 0;
                        let mut total_correct = 0;

                        let start_time = Instant::now();

                        // Validate gold annotations before evaluation (warn but continue)
                        #[cfg(feature = "eval-advanced")]
                        {
                            use crate::eval::validation::validate_ground_truth_entities;
                            let mut total_warnings = 0;
                            for (text, gold) in &test_cases {
                                let validation = validate_ground_truth_entities(text, gold, false);
                                if !validation.is_valid {
                                    eprintln!(
                                        "{} Invalid gold annotations: {}",
                                        color("33", "warning:"),
                                        validation.errors.join("; ")
                                    );
                                }
                                total_warnings += validation.warnings.len();
                            }
                            if total_warnings > 0 {
                                eprintln!(
                                    "{} {} validation warnings in gold annotations",
                                    color("33", "warning:"),
                                    total_warnings
                                );
                            }
                        }

                        for (text, gold) in &test_cases {
                            let entities = m.extract_entities(text, None).unwrap_or_default();

                            total_gold += gold.len();
                            total_pred += entities.len();

                            // Track which predictions have been matched to prevent double-counting
                            let mut matched_pred = vec![false; entities.len()];

                            for gold_entity in gold {
                                // Apply type mapping if available (consistent with matching logic)
                                let gold_type = if let Some(ref mapper) = type_mapper {
                                    mapper.normalize(&gold_entity.original_label)
                                } else {
                                    anno_core::EntityType::from_label(&gold_entity.original_label)
                                };

                                // Use canonical normalization for consistent grouping
                                let gold_type_key =
                                    crate::cli::utils::normalize_entity_type_canonical(
                                        gold_type.as_label(),
                                    );

                                // Track per-type gold counts using normalized type
                                per_type_stats
                                    .entry(gold_type_key.clone())
                                    .or_insert((0, 0, 0))
                                    .0 += 1;

                                // Match: exact span + type match (with flexible type matching)
                                // Find first unmatched prediction that matches
                                let matched = entities.iter().enumerate().any(|(i, e)| {
                                    if matched_pred[i] {
                                        return false; // Already matched
                                    }

                                    let span_match =
                                        e.start == gold_entity.start && e.end == gold_entity.end;
                                    if !span_match {
                                        return false;
                                    }

                                    // Type match with flexible matching
                                    let pred_type_str = e.entity_type.as_label();
                                    let gold_type_str = gold_type.as_label();

                                    // Exact match or flexible match
                                    let type_matches = pred_type_str == gold_type_str
                                        || types_match_flexible(pred_type_str, gold_type_str);

                                    if type_matches {
                                        matched_pred[i] = true; // Mark as matched
                                        return true;
                                    }

                                    false
                                });

                                if matched {
                                    total_correct += 1;
                                    // Track per-type correct count
                                    per_type_stats.entry(gold_type_key).or_insert((0, 0, 0)).2 += 1;
                                }
                            }

                            // Track per-type pred counts
                            // Use canonical normalization so PER/PERSON etc. are grouped together
                            for e in &entities {
                                let raw_type = e.entity_type.as_label().to_uppercase();
                                // Normalize pred type to match gold type normalization
                                let type_key =
                                    crate::cli::utils::normalize_entity_type_canonical(&raw_type);
                                per_type_stats.entry(type_key).or_insert((0, 0, 0)).1 += 1;
                            }
                        }

                        let elapsed = start_time.elapsed();

                        // Handle edge case: if no gold entities and no predictions, that's perfect
                        let (p, r, f1) = if total_gold == 0 && total_pred == 0 {
                            (1.0, 1.0, 1.0)
                        } else {
                            let p = if total_pred > 0 {
                                total_correct as f64 / total_pred as f64
                            } else {
                                0.0
                            };
                            let r = if total_gold > 0 {
                                total_correct as f64 / total_gold as f64
                            } else {
                                0.0
                            };
                            let f1 = if p + r > 0.0 {
                                2.0 * p * r / (p + r)
                            } else {
                                0.0
                            };
                            (p, r, f1)
                        };

                        println!("Results:");
                        println!(
                            "  Gold: {}  Predicted: {}  Correct: {}",
                            total_gold, total_pred, total_correct
                        );
                        println!(
                            "  P: {:.1}%  R: {:.1}%  F1: {:.1}%",
                            p * 100.0,
                            r * 100.0,
                            f1 * 100.0
                        );

                        // Per-entity-type breakdown (sorted by gold count descending)
                        let mut type_entries: Vec<_> = per_type_stats.iter().collect();
                        type_entries.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

                        if !type_entries.is_empty() {
                            println!();
                            println!("Per-type breakdown:");
                            for (type_name, (gold_count, pred_count, correct_count)) in type_entries
                            {
                                if *gold_count == 0 && *pred_count == 0 {
                                    continue;
                                }
                                let type_p = if *pred_count > 0 {
                                    *correct_count as f64 / *pred_count as f64
                                } else {
                                    0.0
                                };
                                let type_r = if *gold_count > 0 {
                                    *correct_count as f64 / *gold_count as f64
                                } else {
                                    0.0
                                };
                                let type_f1 = if type_p + type_r > 0.0 {
                                    2.0 * type_p * type_r / (type_p + type_r)
                                } else {
                                    0.0
                                };
                                println!(
                                    "  {:12} F1={:5.1}%  P={:5.1}%  R={:5.1}%  [gold={} pred={} correct={}]",
                                    type_name,
                                    type_f1 * 100.0,
                                    type_p * 100.0,
                                    type_r * 100.0,
                                    gold_count,
                                    pred_count,
                                    correct_count
                                );
                            }
                        }

                        let ms_per_sent = if !test_cases.is_empty() {
                            elapsed.as_secs_f64() * 1000.0 / test_cases.len() as f64
                        } else {
                            0.0
                        };
                        println!();
                        println!(
                            "  Time: {:.1}s ({:.1}ms/sent)",
                            elapsed.as_secs_f64(),
                            ms_per_sent
                        );
                        println!();
                    }
                    EvalTask::Coref => {
                        #[cfg(not(feature = "eval-advanced"))]
                        {
                            return Err("Coreference evaluation requires --features eval-advanced"
                                .to_string());
                        }
                        #[cfg(feature = "eval-advanced")]
                        {
                            use crate::eval::coref_resolver::SimpleCorefResolver;
                            use crate::eval::loader::DatasetLoader;

                            if dataset == "synthetic" {
                                return Err("Coreference evaluation requires a real dataset (e.g., gap, preco, litbank)".to_string());
                            }

                            // Reuse parsed result (preserves original error message)
                            let dataset_id: DatasetId =
                                parsed_dataset_result.clone().map_err(|e| e)?;

                            if !dataset_id.is_coreference() {
                                return Err(format!("Dataset '{}' is not a coreference dataset. Use: gap, preco, or litbank", dataset));
                            }

                            let loader = DatasetLoader::new()
                                .map_err(|e| format!("Failed to init dataset loader: {}", e))?;

                            println!();
                            println!(
                                "Evaluating coreference resolution on {} dataset...",
                                dataset_id.name()
                            );
                            println!("Loading dataset (may download if not cached)...");

                            let gold_docs =
                                loader.load_or_download_coref(dataset_id).map_err(|e| {
                                    format!("Failed to load coreference dataset: {}", e)
                                })?;

                            println!("  Documents: {}", gold_docs.len());
                            println!();

                            let resolver = SimpleCorefResolver::default();
                            let mut all_pred_chains: Vec<Vec<crate::eval::coref::CorefChain>> =
                                Vec::new();
                            let mut all_gold_chains: Vec<&[crate::eval::coref::CorefChain]> =
                                Vec::new();
                            let start_time = Instant::now();

                            for doc in &gold_docs {
                                let text = doc.text.as_str();
                                all_gold_chains.push(&doc.chains);

                                // Extract entities using NER
                                let entities = m.extract_entities(text, None).unwrap_or_default();

                                // Resolve coreference
                                let pred_chains = resolver.resolve_to_chains(&entities);
                                all_pred_chains.push(pred_chains);
                            }

                            let elapsed = start_time.elapsed();

                            // Build document pairs
                            let document_pairs: Vec<_> = all_pred_chains
                                .iter()
                                .zip(all_gold_chains.iter())
                                .map(|(pred, gold)| (pred.as_slice(), *gold))
                                .collect();

                            // Compute aggregate metrics
                            let results =
                                crate::eval::coref_metrics::AggregateCorefEvaluation::compute(
                                    &document_pairs,
                                );

                            println!("Results:");
                            println!("  CoNLL F1: {:.3}", results.mean.conll_f1);
                            println!(
                                "  MUC: P={:.3} R={:.3} F1={:.3}",
                                results.mean.muc.precision,
                                results.mean.muc.recall,
                                results.mean.muc.f1
                            );
                            println!(
                                "  B³: P={:.3} R={:.3} F1={:.3}",
                                results.mean.b_cubed.precision,
                                results.mean.b_cubed.recall,
                                results.mean.b_cubed.f1
                            );
                            println!(
                                "  CEAF-e: P={:.3} R={:.3} F1={:.3}",
                                results.mean.ceaf_e.precision,
                                results.mean.ceaf_e.recall,
                                results.mean.ceaf_e.f1
                            );
                            println!(
                                "  LEA: P={:.3} R={:.3} F1={:.3}",
                                results.mean.lea.precision,
                                results.mean.lea.recall,
                                results.mean.lea.f1
                            );
                            println!(
                                "  BLANC: P={:.3} R={:.3} F1={:.3}",
                                results.mean.blanc.precision,
                                results.mean.blanc.recall,
                                results.mean.blanc.f1
                            );
                            println!("  Documents: {}", results.num_documents);
                            println!("  Time: {:.1}s", elapsed.as_secs_f64());
                            println!();
                        }
                    }
                    EvalTask::Relation => {
                        #[cfg(not(feature = "eval-advanced"))]
                        {
                            return Err(
                                "Relation extraction evaluation requires --features eval-advanced"
                                    .to_string(),
                            );
                        }
                        #[cfg(feature = "eval-advanced")]
                        {
                            use crate::backends::inference::RelationExtractor;
                            use crate::eval::loader::DatasetLoader;
                            use crate::eval::relation::{
                                evaluate_relations, RelationEvalConfig, RelationPrediction,
                            };

                            if dataset == "synthetic" {
                                return Err("Relation extraction evaluation requires a real dataset (e.g., docred, retacred)".to_string());
                            }

                            // Reuse parsed result (preserves original error message)
                            let dataset_id: DatasetId =
                                parsed_dataset_result.clone().map_err(|e| e)?;

                            if !dataset_id.is_relation_extraction() {
                                return Err(format!("Dataset '{}' is not a relation extraction dataset. Use: docred or retacred", dataset));
                            }

                            let loader = DatasetLoader::new()
                                .map_err(|e| format!("Failed to init dataset loader: {}", e))?;

                            println!();
                            println!(
                                "Evaluating relation extraction on {} dataset...",
                                dataset_id.name()
                            );
                            println!("Loading dataset (may download if not cached)...");

                            let gold_docs = loader
                                .load_or_download_relation(dataset_id)
                                .map_err(|e| format!("Failed to load relation dataset: {}", e))?;

                            println!("  Documents: {}", gold_docs.len());
                            println!();

                            // Try to use RelationExtractor if available (e.g., GLiNER2)
                            // Otherwise fall back to entity-pair heuristic

                            // Collect entity types and relation types from gold data
                            let mut entity_types = std::collections::HashSet::new();
                            let mut relation_types = std::collections::HashSet::new();
                            for doc in &gold_docs {
                                for rel in &doc.relations {
                                    entity_types.insert(rel.head_type.clone());
                                    entity_types.insert(rel.tail_type.clone());
                                    relation_types.insert(rel.relation_type.clone());
                                }
                            }

                            let entity_types_vec: Vec<&str> =
                                entity_types.iter().map(|s| s.as_str()).collect_vec();
                            let relation_types_vec: Vec<&str> =
                                relation_types.iter().map(|s| s.as_str()).collect_vec();

                            println!("  Entity types: {}", entity_types_vec.join(", "));
                            println!(
                                "  Relation types: {} ({} total)",
                                relation_types_vec.len(),
                                relation_types_vec.iter().take(5).join(", ")
                            );
                            println!();

                            // Check if we can use GLiNER2 for relation extraction
                            let use_relation_extractor: Option<Box<dyn RelationExtractor>> = {
                                #[cfg(feature = "onnx")]
                                {
                                    // Try to create GLiNER2 multitask model for relation extraction
                                    if let Ok(gliner2) =
                                        crate::backends::gliner2::GLiNER2Onnx::from_pretrained(
                                            "onnx-community/gliner-multitask-large-v0.5",
                                        )
                                    {
                                        Some(Box::new(gliner2) as Box<dyn RelationExtractor>)
                                    } else {
                                        None
                                    }
                                }
                                #[cfg(not(feature = "onnx"))]
                                {
                                    None
                                }
                            };

                            let mut all_gold = Vec::new();
                            let mut all_pred = Vec::new();
                            let start_time = Instant::now();

                            if let Some(ref rel_extractor) = use_relation_extractor {
                                println!("{} Using GLiNER2 RelationExtractor", color("32", "[OK]"));
                                println!("  Note: This uses regex matching on text, not a neural relation model.");
                                println!();

                                for doc in &gold_docs {
                                    let text = doc.text.as_str();
                                    all_gold.extend(doc.relations.clone());

                                    // Use RelationExtractor
                                    match rel_extractor.extract_with_relations(
                                        text,
                                        &entity_types_vec,
                                        &relation_types_vec,
                                        0.5,
                                    ) {
                                        Ok(result) => {
                                            // Convert RelationTriples to RelationPredictions
                                            for triple in &result.relations {
                                                if let Some(pred) =
                                                    RelationPrediction::from_triple_with_entities(
                                                        triple,
                                                        &result.entities,
                                                    )
                                                {
                                                    all_pred.push(pred);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "{} Relation extraction failed: {}",
                                                color("33", "warning:"),
                                                e
                                            );
                                            // Fall back to entity-pair heuristic for this document
                                            let entities =
                                                m.extract_entities(text, None).unwrap_or_default();
                                            all_pred.extend(create_entity_pair_relations(
                                                &entities,
                                                text,
                                                &relation_types_vec,
                                            ));
                                        }
                                    }
                                }
                            } else {
                                println!("{} Using entity-pair heuristic (GLiNER2 multitask not available)", color("33", "!"));
                                println!();

                                for doc in &gold_docs {
                                    let text = doc.text.as_str();
                                    all_gold.extend(doc.relations.clone());

                                    // Extract entities using NER
                                    let entities =
                                        m.extract_entities(text, None).unwrap_or_default();

                                    // Create relation predictions from entity pairs
                                    all_pred.extend(create_entity_pair_relations(
                                        &entities,
                                        text,
                                        &relation_types_vec,
                                    ));
                                }
                            }

                            let elapsed = start_time.elapsed();

                            // Evaluate relations
                            // Note: require_entity_type_match=false because entity types may differ
                            // (e.g., gold uses "person" but pred uses "Person", or "PER" vs "PERSON")
                            let config = RelationEvalConfig {
                                overlap_threshold: 0.5,
                                require_entity_type_match: false, // More lenient for evaluation
                                directed_relations: true,
                            };
                            let metrics = evaluate_relations(&all_gold, &all_pred, &config);

                            // Output results (human-readable by default)
                            println!();
                            println!("{}", color("1;36", "======================================================================="));
                            println!(
                                "  {}  model={}  time={:.1}s",
                                color("1;36", "RELATION EXTRACTION EVALUATION"),
                                model.name(),
                                elapsed.as_secs_f64()
                            );
                            println!("{}", color("1;36", "======================================================================="));
                            println!();
                            println!("{}", metrics.to_string_human(false)); // verbose=false for now
                            println!();
                        }
                    }
                }
            }
            #[cfg(not(feature = "eval"))]
            {
                let _ = (dataset, model, task);
                return Err("Dataset evaluation requires --features eval".to_string());
            }
        }
        DatasetAction::Export(args) => {
            super::export::run(args)?;
        }
        DatasetAction::Import(args) => {
            super::import::run(args)?;
        }
        DatasetAction::Context(args) => {
            super::context::run(args)?;
        }
    }

    Ok(())
}

/// Run the info subcommand
fn run_info(dataset: &str) -> Result<(), String> {
    println!();

    #[cfg(feature = "eval")]
    {
        use crate::eval::dataset_registry::DatasetId as RegistryDatasetId;
        use crate::eval::loader::DatasetId as LoadableDatasetId;

        // Try to find in registry (by display name or variant name)
        let registry_match = RegistryDatasetId::all()
            .iter()
            .find(|d| {
                // Match by display name (case-insensitive)
                d.name().eq_ignore_ascii_case(dataset)
                    // Or by variant name (e.g., "BroadTwitterCorpus")
                    || format!("{:?}", d).eq_ignore_ascii_case(dataset)
            })
            .copied();

        // Try to parse as loadable dataset
        let loadable_match = dataset.parse::<LoadableDatasetId>().ok();

        if let Some(registry_id) = registry_match {
            // Show registry metadata
            println!(
                "{}",
                color("1;36", &format!("Dataset: {}", registry_id.name()))
            );
            println!();

            // Basic info
            println!("  Description: {}", registry_id.description());
            println!("  Language:    {}", registry_id.language());
            println!("  Domain:      {}", registry_id.domain());

            // Optional metadata
            if let Some(year) = registry_id.year() {
                println!("  Year:        {}", year);
            }
            if let Some(citation) = registry_id.citation() {
                println!("  Citation:    {}", citation);
            }
            if let Some(license) = registry_id.license() {
                println!("  License:     {}", license);
            }
            if let Some(paper_url) = registry_id.paper_url() {
                println!("  Paper:       {}", paper_url);
            }
            if let Some(size_hint) = registry_id.size_hint() {
                println!("  Size:        {}", size_hint);
            }

            // Entity types
            let entity_types = registry_id.entity_types();
            if !entity_types.is_empty() {
                println!("  Entity types: {}", entity_types.join(", "));
            }

            // Task capabilities
            println!();
            println!("  Tasks:");
            if registry_id.is_ner() {
                println!("    - Named Entity Recognition");
            }
            if registry_id.is_coreference() {
                println!("    - Coreference Resolution");
            }
            if registry_id.is_biomedical() {
                println!("    (Biomedical domain)");
            }

            // Check if loadable
            println!();
            if loadable_match.is_some() {
                println!(
                    "  Status: {} (can be downloaded)",
                    color("1;32", "Loadable")
                );

                // Try to load and show stats if eval-advanced is enabled
                #[cfg(feature = "eval-advanced")]
                {
                    use crate::eval::loader::DatasetLoader;
                    if let Ok(loadable_id) = dataset.parse::<LoadableDatasetId>() {
                        let loader = DatasetLoader::new()
                            .map_err(|e| format!("Failed to create loader: {}", e))?;

                        match loader.load(loadable_id) {
                            Ok(loaded) => {
                                let stats = loaded.stats();
                                println!();
                                println!("  Loaded Statistics:");
                                println!("    Sentences: {}", stats.sentences);
                                println!("    Tokens:    {}", stats.tokens);
                                println!("    Entities:  {}", stats.entities);
                                if stats.sentences > 0 {
                                    println!(
                                        "    Avg entities/sentence: {:.2}",
                                        stats.entities as f64 / stats.sentences as f64
                                    );
                                }
                            }
                            Err(e) => {
                                println!("  (Could not load: {})", e);
                            }
                        }
                    }
                }
            } else {
                let access_status = registry_id.access_status();
                println!(
                    "  Status: {} ({})",
                    color("1;33", "Not loadable"),
                    access_status.as_str()
                );
            }
        } else {
            return Err(format!(
                "Unknown dataset '{}'. Use 'anno dataset list' to see available datasets.",
                dataset
            ));
        }
    }

    #[cfg(not(feature = "eval"))]
    {
        println!("Dataset: {}", dataset);
        println!();
        println!("Note: Full dataset info requires --features eval");
    }

    println!();
    Ok(())
}

/// Run the list subcommand
fn run_list(
    task_filter: Option<String>,
    domain_filter: Option<String>,
    loadable_only: bool,
    verbose: bool,
) -> Result<(), String> {
    println!();
    println!("{}", color("1;36", "Available Datasets"));
    println!();

    #[cfg(feature = "eval")]
    {
        use crate::eval::dataset_registry::DatasetId as RegistryDatasetId;
        use crate::eval::loader::DatasetId as LoadableDatasetId;

        if loadable_only {
            // Show only loadable datasets
            println!(
                "  {} loadable datasets (can be downloaded and parsed):",
                LoadableDatasetId::all().len()
            );
            println!();

            for id in LoadableDatasetId::all() {
                let name = id.name();
                if let Some(ref task) = task_filter {
                    // Use the dataset's categorization (loader method or registry fallback)
                    let is_coref = id.is_coreference();
                    // NER is the default if not specifically coref
                    let is_ner = id.to_registry().is_ner() || !is_coref;

                    match task.as_str() {
                        "coref" if !is_coref => continue,
                        "ner" if !is_ner => continue,
                        _ => {}
                    }
                }

                if verbose {
                    let citation = id.citation().unwrap_or("N/A");
                    let license = id.license().unwrap_or("Unknown");
                    let year = id
                        .year()
                        .map(|y| y.to_string())
                        .unwrap_or_else(|| "N/A".to_string());
                    println!("    {:<20} [{:>4}] {} ({})", name, year, citation, license);
                } else {
                    println!("    {}", name);
                }
            }
        } else {
            // Show registry (full catalog)
            let all_datasets: Vec<_> = RegistryDatasetId::all().iter().collect();
            println!(
                "  {} datasets in registry ({} loadable):",
                all_datasets.len(),
                LoadableDatasetId::all().len()
            );
            println!();

            // Group by domain if no filter
            if domain_filter.is_none() && task_filter.is_none() {
                // Just show counts by category
                let ner_count = all_datasets.iter().filter(|d| d.is_ner()).count();
                let coref_count = all_datasets.iter().filter(|d| d.is_coreference()).count();
                let bio_count = all_datasets.iter().filter(|d| d.is_biomedical()).count();

                println!("    NER datasets:           {}", ner_count);
                println!("    Coreference datasets:   {}", coref_count);
                println!("    Biomedical datasets:    {}", bio_count);
                println!();
                println!("  Use --loadable to see only downloadable datasets");
                println!("  Use --task ner/coref to filter by task");
                println!("  Use --verbose for more details");
            } else {
                // Filter and display
                for dataset in &all_datasets {
                    // Apply task filter
                    if let Some(ref task) = task_filter {
                        match task.as_str() {
                            "ner" if !dataset.is_ner() => continue,
                            "coref" if !dataset.is_coreference() => continue,
                            _ => {}
                        }
                    }

                    // Apply domain filter
                    if let Some(ref domain) = domain_filter {
                        let dataset_domain = dataset.domain().to_lowercase();
                        if !dataset_domain.contains(&domain.to_lowercase()) {
                            continue;
                        }
                    }

                    if verbose {
                        let citation = dataset.citation().unwrap_or("N/A");
                        let year = dataset
                            .year()
                            .map(|y| y.to_string())
                            .unwrap_or_else(|| "----".to_string());
                        println!("    {:<25} [{:>4}] {}", dataset.name(), year, citation);
                    } else {
                        println!("    {}", dataset.name());
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "eval"))]
    {
        let _ = (task_filter, domain_filter, loadable_only, verbose);
        println!("  Synthetic (always available):");
        println!("    - synthetic   : Generated test cases");
        println!("    - robustness  : Adversarial perturbations");
        println!();
        println!("  Note: Full dataset catalog requires --features eval");
    }

    println!();
    Ok(())
}
