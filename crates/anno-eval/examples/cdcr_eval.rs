// Run CDCR evaluation on cached coref datasets.
//
// Usage: cargo run -p anno-eval --example cdcr_eval --features "eval"
//
// Env:
//   ANNO_CDCR_DATASETS=ECBPlus,GAP  -- filter datasets (default: all cached coref)
//   ANNO_CDCR_MAX_DOCS=50           -- limit docs per dataset

use anno_eval::eval::cdcr::{CrossDocCluster, Document};
use anno_eval::eval::cluster_encoder::{CosineMergeScorer, HeuristicClusterEncoder};
use anno_eval::eval::coref::CorefDocument;
use anno_eval::eval::cross_context_eval::{
    evaluate_cross_document, CrossContextEvalConfig, Topic,
};
use anno_eval::eval::loader::{DatasetLoader, DatasetId};
use std::collections::HashMap;
use std::time::Instant;

/// Evaluation configuration preset.
#[derive(Debug, Clone)]
struct EvalConfig {
    name: &'static str,
    use_gold_mentions: bool,
    use_gold_clusters: bool,
    thresholds: Vec<f32>,
}

fn main() {
    let dataset_filter: Option<Vec<String>> = std::env::var("ANNO_CDCR_DATASETS")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect());
    let max_docs: Option<usize> = std::env::var("ANNO_CDCR_MAX_DOCS")
        .ok()
        .and_then(|v| v.parse().ok());

    let loader = match DatasetLoader::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to create DatasetLoader: {}", e);
            std::process::exit(1);
        }
    };

    // Coref datasets to evaluate
    let coref_ids: Vec<DatasetId> = DatasetId::all()
        .iter()
        .copied()
        .filter(|id| {
            if !id.is_coreference() || !id.is_automatable() {
                return false;
            }
            if let Some(ref filter) = dataset_filter {
                let name = format!("{:?}", id);
                return filter.iter().any(|f| {
                    name.eq_ignore_ascii_case(f) || id.name().eq_ignore_ascii_case(f)
                });
            }
            true
        })
        .collect();

    let configs = vec![
        EvalConfig {
            name: "oracle",
            use_gold_mentions: true,
            use_gold_clusters: true,
            thresholds: vec![0.5],
        },
        EvalConfig {
            name: "gold_mentions",
            use_gold_mentions: true,
            use_gold_clusters: false,
            thresholds: vec![0.3, 0.5, 0.7],
        },
        EvalConfig {
            name: "end_to_end",
            use_gold_mentions: false,
            use_gold_clusters: false,
            thresholds: vec![0.3, 0.5, 0.7],
        },
    ];

    println!("## CDCR Evaluation Results\n");
    println!(
        "| {:<10} | {:<14} | {:>5} | {:>5} | {:>5} | {:>5} | {:>6} | {:>4} | {:>5} | {:>5} | {:>6} |",
        "Dataset", "Config", "Thr", "CoNLL", "MUC", "B3", "CEAF", "Top", "Gold", "Pred", "ms"
    );
    println!(
        "|{:-<12}|{:-<16}|{:-<7}|{:-<7}|{:-<7}|{:-<7}|{:-<8}|{:-<6}|{:-<7}|{:-<7}|{:-<8}|",
        "", "", "", "", "", "", "", "", "", "", ""
    );

    for &id in &coref_ids {
        // Try to load (skip if not cached)
        let docs = match loader.load_or_download_coref(id) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Skipping {:?}: {}", id, e);
                continue;
            }
        };

        let docs = if let Some(max) = max_docs {
            if docs.len() > max {
                docs.into_iter().take(max).collect()
            } else {
                docs
            }
        } else {
            docs
        };

        for config in &configs {
            for &threshold in &config.thresholds {
                match run_eval(id, &docs, config, threshold) {
                    Ok(row) => println!("{}", row),
                    Err(e) => {
                        eprintln!(
                            "  Error: {:?} / {} / {}: {}",
                            id, config.name, threshold, e
                        );
                    }
                }
            }
        }
    }
}

fn run_eval(
    id: DatasetId,
    docs: &[CorefDocument],
    config: &EvalConfig,
    threshold: f32,
) -> Result<String, anno_eval::Error> {
    let start = Instant::now();

    // Group docs by topic
    let mut topics_map: HashMap<String, Vec<&CorefDocument>> = HashMap::new();
    for doc in docs {
        let topic_key = doc
            .doc_id
            .as_deref()
            .and_then(|id| id.split('_').next())
            .unwrap_or("default")
            .to_string();
        topics_map.entry(topic_key).or_default().push(doc);
    }

    // Build Topic objects (same pattern as task_evaluator)
    let mut topics: Vec<Topic> = Vec::new();
    let mut topic_keys: Vec<_> = topics_map.keys().cloned().collect();
    topic_keys.sort();

    for topic_key in &topic_keys {
        let coref_docs = &topics_map[topic_key];
        let mut topic = Topic::new(topic_key);
        let mut chain_to_mentions: HashMap<String, Vec<(String, usize)>> = HashMap::new();

        for coref_doc in coref_docs {
            let doc_id = coref_doc
                .doc_id
                .clone()
                .unwrap_or_else(|| format!("doc_{}", topic.documents.len()));

            let mut entities: Vec<anno::Entity> = Vec::new();
            for (chain_idx, chain) in coref_doc.chains.iter().enumerate() {
                for mention in &chain.mentions {
                    let et = mention
                        .entity_type
                        .as_deref()
                        .map(|t| {
                            let tl = t.to_lowercase();
                            if tl.contains("person") {
                                anno::EntityType::Person
                            } else if tl.contains("loc") || tl.contains("place") {
                                anno::EntityType::Location
                            } else if tl.contains("org") {
                                anno::EntityType::Organization
                            } else {
                                anno::EntityType::Other(t.to_string())
                            }
                        })
                        .unwrap_or(anno::EntityType::Other("mention".to_string()));

                    let entity_idx = entities.len();
                    entities.push(anno::Entity::new(
                        &mention.text,
                        et,
                        mention.start,
                        mention.end,
                        1.0,
                    ));

                    // Use chain cluster_id for cross-doc grouping (ECB+ encodes cross-doc identity in chain IDs)
                    let chain_key = if let Some(cid) = chain.cluster_id {
                        format!("{}", cid.get())
                    } else {
                        format!("{}_{}", topic_key, chain_idx)
                    };
                    chain_to_mentions
                        .entry(chain_key)
                        .or_default()
                        .push((doc_id.clone(), entity_idx));
                }
            }

            let cdcr_doc = Document::new(&doc_id, &coref_doc.text).with_entities(entities);
            topic.add_document(cdcr_doc);
        }

        // Build gold CrossDocClusters
        for (_chain_key, mentions) in &chain_to_mentions {
            if mentions.len() < 2 {
                continue;
            }
            let mut cluster =
                CrossDocCluster::new(topic.gold_clusters.len() as u64, "");
            cluster.mentions = mentions.clone();
            topic.add_gold_cluster(cluster);
        }

        topics.push(topic);
    }

    let encoder = HeuristicClusterEncoder::new(64);
    let scorer = CosineMergeScorer::new(threshold);
    let eval_config = CrossContextEvalConfig {
        merge_threshold: threshold,
        use_gold_mentions: config.use_gold_mentions,
        use_gold_clusters: config.use_gold_clusters,
        ..Default::default()
    };

    let results = evaluate_cross_document(&topics, encoder, scorer, &eval_config)?;
    let elapsed = start.elapsed();

    Ok(format!(
        "| {:<10} | {:<14} | {:>5.1} | {:>5.1} | {:>5.1} | {:>5.1} | {:>6.1} | {:>4} | {:>5} | {:>5} | {:>6} |",
        id.name(),
        config.name,
        threshold,
        results.conll_f1 * 100.0,
        results.muc.f1 * 100.0,
        results.b_cubed.f1 * 100.0,
        results.ceaf_e.f1 * 100.0,
        topics.len(),
        results.num_gold_clusters,
        results.num_pred_clusters,
        elapsed.as_millis(),
    ))
}
