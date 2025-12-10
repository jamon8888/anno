//! Export coreference data for box-coref training.
//!
//! Converts anno's coreference datasets (PreCo, GAP, LitBank) into the JSONL
//! format expected by box-coref's training pipeline.
//!
//! # Output Format
//!
//! Each line is a JSON object representing a containment or relation pair:
//! ```json
//! {"child": "mention_text", "parent": "CLUSTER_0_doc1", "relation": "contained_in", ...}
//! ```
//!
//! # Usage
//!
//! ```bash
//! cargo run --example export_coref_for_box_training -- --dataset preco --output exports/
//! cargo run --example export_coref_for_box_training -- --dataset gap --output exports/
//! ```

use anno::eval::coref::{CorefChain, CorefDocument, Mention};
use serde::Serialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// A containment or relation pair for box-coref training.
#[derive(Debug, Serialize)]
struct BoxCorefPair {
    child: String,
    parent: String,
    relation: String,
    child_type: String,
    parent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_id: Option<String>,
}

/// Export a coreference document to box-coref training format.
fn export_document(doc: &CorefDocument) -> Vec<BoxCorefPair> {
    let mut pairs = Vec::new();
    let mut entity_types: HashSet<String> = HashSet::new();
    let doc_id_str = doc.doc_id.clone().unwrap_or_else(|| "unknown".to_string());

    // For each cluster, create mention-to-cluster containment pairs
    for (cluster_idx, chain) in doc.chains.iter().enumerate() {
        let cluster_id = format!("CLUSTER_{}_{}", cluster_idx, doc_id_str);

        for mention in &chain.mentions {
            // Mention contained in cluster
            pairs.push(BoxCorefPair {
                child: mention.text.clone(),
                parent: cluster_id.clone(),
                relation: "contained_in".to_string(),
                child_type: "MENTION".to_string(),
                parent_type: "COREFERENCE_CLUSTER".to_string(),
                doc_id: Some(doc_id_str.clone()),
            });

            // If mention has entity type, record type membership
            if let Some(ref etype) = mention.entity_type {
                pairs.push(BoxCorefPair {
                    child: mention.text.clone(),
                    parent: etype.clone(),
                    relation: "is_a".to_string(),
                    child_type: "MENTION".to_string(),
                    parent_type: "ENTITY_TYPE".to_string(),
                    doc_id: Some(doc_id_str.clone()),
                });
                entity_types.insert(etype.clone());
            }
        }
    }

    // Generate negative pairs (disjoint clusters)
    for (i, chain_i) in doc.chains.iter().enumerate() {
        for (j, chain_j) in doc.chains.iter().enumerate() {
            if i >= j {
                continue;
            }

            // Sample some mention pairs to avoid quadratic explosion
            let sample_size = std::cmp::min(3, chain_i.mentions.len());
            for mention_i in chain_i.mentions.iter().take(sample_size) {
                for mention_j in chain_j.mentions.iter().take(sample_size) {
                    pairs.push(BoxCorefPair {
                        child: mention_i.text.clone(),
                        parent: mention_j.text.clone(),
                        relation: "disjoint_from".to_string(),
                        child_type: "MENTION".to_string(),
                        parent_type: "MENTION".to_string(),
                        doc_id: Some(doc_id_str.clone()),
                    });
                }
            }
        }
    }

    // Add type hierarchy (all entity types are subtypes of ENTITY)
    for etype in entity_types {
        pairs.push(BoxCorefPair {
            child: etype,
            parent: "ENTITY".to_string(),
            relation: "is_a".to_string(),
            child_type: "ENTITY_TYPE".to_string(),
            parent_type: "UNIVERSAL_TYPE".to_string(),
            doc_id: None,
        });
    }

    pairs
}

/// Create synthetic demo documents for testing the export.
fn create_demo_documents() -> Vec<CorefDocument> {
    vec![
        CorefDocument {
            doc_id: Some("doc1".to_string()),
            text: "Barack Obama was born in Hawaii. He later became president.".to_string(),
            chains: vec![
                CorefChain::new(vec![
                    Mention::new("Barack Obama", 0, 12),
                    Mention::new("He", 33, 35),
                ]),
                CorefChain::new(vec![Mention::new("Hawaii", 25, 31)]),
            ],
            includes_singletons: false,
        },
        CorefDocument {
            doc_id: Some("doc2".to_string()),
            text: "Google acquired DeepMind. The company is investing in AI.".to_string(),
            chains: vec![
                CorefChain::new(vec![
                    Mention::new("Google", 0, 6),
                    Mention::new("The company", 25, 36),
                ]),
                CorefChain::new(vec![Mention::new("DeepMind", 16, 24)]),
            ],
            includes_singletons: false,
        },
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Parse simple arguments
    let mut output_dir = PathBuf::from("exports");
    let mut dataset = "demo".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output" => {
                output_dir = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--dataset" => {
                dataset = args[i + 1].clone();
                i += 2;
            }
            _ => i += 1,
        }
    }

    // Create output directory
    std::fs::create_dir_all(&output_dir)?;

    // Get documents
    let documents = match dataset.as_str() {
        "demo" => {
            eprintln!("Using demo documents (use --dataset preco|gap for real data)");
            create_demo_documents()
        }
        "preco" => {
            // PreCo requires manual download or HuggingFace datasets library
            // Direct URL returns just a webpage, not data
            eprintln!("PreCo dataset not directly available via URL.");
            eprintln!("Options:");
            eprintln!("  1. Download manually from: https://preschool-lab.github.io/PreCo/");
            eprintln!("  2. Use HuggingFace datasets: huggingface-cli download coref-data/preco");
            eprintln!("  3. Use GAP dataset instead (--dataset gap)");
            eprintln!();
            eprintln!("Using demo data for now...");
            create_demo_documents()
        }
        "gap" => {
            #[cfg(feature = "eval-advanced")]
            {
                use anno::eval::coref_loader::CorefLoader;
                use anno::eval::loader::DatasetId;
                use anno::eval::loader::DatasetLoader;

                eprintln!("Loading GAP dataset...");
                let loader =
                    DatasetLoader::new().map_err(|e| format!("Failed to create loader: {}", e))?;

                // Download if not cached
                if !loader.is_cached(DatasetId::GAP) {
                    eprintln!("Downloading GAP dataset...");
                    let _ = loader
                        .load_or_download(DatasetId::GAP)
                        .map_err(|e| format!("Failed to download GAP: {}", e))?;
                }

                let coref_loader = CorefLoader::new()
                    .map_err(|e| format!("Failed to create coref loader: {}", e))?;
                coref_loader
                    .load_gap()
                    .map_err(|e| format!("Failed to load GAP: {}", e))?
            }
            #[cfg(not(feature = "eval-advanced"))]
            {
                eprintln!("GAP requires --features eval-advanced, using demo");
                create_demo_documents()
            }
        }
        "litbank" => {
            eprintln!("LitBank not yet implemented, using demo");
            create_demo_documents()
        }
        _ => {
            eprintln!("Unknown dataset: {}", dataset);
            std::process::exit(1);
        }
    };

    // Export to JSONL
    let output_path = output_dir.join(format!("{}_coref_training.jsonl", dataset));
    let file = File::create(&output_path)?;
    let mut writer = BufWriter::new(file);

    let mut total_pairs = 0;
    for doc in &documents {
        let pairs = export_document(doc);
        total_pairs += pairs.len();
        for pair in pairs {
            serde_json::to_writer(&mut writer, &pair)?;
            writeln!(writer)?;
        }
    }

    eprintln!(
        "Exported {} documents with {} pairs to {:?}",
        documents.len(),
        total_pairs,
        output_path
    );

    // Print statistics
    eprintln!("\nStatistics:");
    eprintln!("  Documents: {}", documents.len());
    eprintln!("  Total pairs: {}", total_pairs);
    eprintln!(
        "  Avg pairs/doc: {:.1}",
        total_pairs as f64 / documents.len() as f64
    );

    Ok(())
}
