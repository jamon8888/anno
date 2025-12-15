//! Joint model training from real coreference data.
//!
//! Demonstrates training the Durrett & Klein joint model using
//! coreference annotations from a real dataset.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p anno --example joint_training_from_dataset --features eval
//! ```

use std::collections::HashMap;

// Minimal types for demonstration (avoiding full dependency on eval feature)
#[derive(Debug, Clone)]
struct SimpleMention {
    text: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct SimpleChain {
    mentions: Vec<SimpleMention>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SimpleDocument {
    text: String,
    chains: Vec<SimpleChain>,
}

// Joint model types from learning.rs (simplified for standalone example)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
enum EntityType {
    Person,
    Organization,
    Location,
    Other,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct JointMention {
    idx: usize,
    text: String,
    start: usize,
    end: usize,
    entity_type: Option<EntityType>,
    head_word: String,
}

#[derive(Debug, Clone, Default)]
struct JointWeights {
    new_cluster_bias: f64,
    distance_decay: f64,
    string_match: f64,
    type_match: f64,
    type_mismatch: f64,
}

#[derive(Debug, Clone)]
struct TrainingConfig {
    learning_rate: f64,
    epochs: usize,
    #[allow(dead_code)]
    l1_reg: f64,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            epochs: 10,
            l1_reg: 0.001,
        }
    }
}

#[derive(Debug, Clone)]
struct TrainingExample {
    mentions: Vec<JointMention>,
    gold_antecedents: HashMap<usize, Option<usize>>, // mention_idx -> antecedent_idx (None = new cluster)
}

/// Convert a coref document to training examples for the joint model.
fn document_to_training_example(doc: &SimpleDocument) -> TrainingExample {
    let mut mentions = Vec::new();
    let mut gold_antecedents = HashMap::new();

    // Collect all mentions from all chains
    let mut all_mentions: Vec<(usize, &SimpleMention)> = doc
        .chains
        .iter()
        .enumerate()
        .flat_map(|(chain_idx, chain)| chain.mentions.iter().map(move |m| (chain_idx, m)))
        .collect();

    // Sort by document position
    all_mentions.sort_by_key(|(_, m)| m.start);

    // Build gold antecedents (first in chain = NEW, rest = previous in chain)
    let mut chain_last_mention: HashMap<usize, usize> = HashMap::new();

    for (idx, (chain_idx, mention)) in all_mentions.iter().enumerate() {
        let head = extract_head(&mention.text);

        mentions.push(JointMention {
            idx,
            text: mention.text.clone(),
            start: mention.start,
            end: mention.end,
            entity_type: None, // Would come from NER labels
            head_word: head,
        });

        // If there's a previous mention in this chain, link to it
        let antecedent = chain_last_mention.get(chain_idx).copied();
        gold_antecedents.insert(idx, antecedent);
        chain_last_mention.insert(*chain_idx, idx);
    }

    TrainingExample {
        mentions,
        gold_antecedents,
    }
}

/// Simple head extraction (last word for nominal mentions).
fn extract_head(text: &str) -> String {
    text.split_whitespace()
        .last()
        .unwrap_or(text)
        .to_lowercase()
}

/// Compute score for a (mention, antecedent) pair.
fn score_pair(
    mention: &JointMention,
    antecedent: Option<&JointMention>,
    weights: &JointWeights,
) -> f64 {
    match antecedent {
        None => weights.new_cluster_bias,
        Some(ant) => {
            let mut score = 0.0;

            // Distance decay
            let distance = mention.idx - ant.idx;
            score += weights.distance_decay * (distance as f64).ln_1p();

            // String match
            if mention.head_word == ant.head_word {
                score += weights.string_match;
            }

            // Type compatibility (if types are known)
            match (&mention.entity_type, &ant.entity_type) {
                (Some(t1), Some(t2)) if t1 == t2 => score += weights.type_match,
                (Some(_), Some(_)) => score += weights.type_mismatch,
                _ => {}
            }

            score
        }
    }
}

/// Log-sum-exp for numerical stability.
fn log_sum_exp(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NEG_INFINITY;
    }
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if max.is_infinite() {
        return max;
    }
    max + values.iter().map(|&v| (v - max).exp()).sum::<f64>().ln()
}

/// Compute softmax probabilities.
fn softmax(logits: &[f64]) -> Vec<f64> {
    let lse = log_sum_exp(logits);
    logits.iter().map(|&l| (l - lse).exp()).collect()
}

/// Train the joint model on a set of examples.
fn train(examples: &[TrainingExample], config: &TrainingConfig) -> JointWeights {
    let mut weights = JointWeights::default();
    let mut grad_squared = JointWeights::default(); // For AdaGrad

    println!(
        "Training on {} examples for {} epochs",
        examples.len(),
        config.epochs
    );
    println!();

    for epoch in 0..config.epochs {
        let mut total_loss = 0.0;

        for example in examples {
            // Forward pass: compute scores and loss
            let mut loss = 0.0;

            for mention in &example.mentions {
                let gold_ant = example
                    .gold_antecedents
                    .get(&mention.idx)
                    .copied()
                    .flatten();

                // Build candidate set (all prior mentions + NEW)
                let mut candidates: Vec<Option<&JointMention>> = vec![None];
                for prev in &example.mentions[..mention.idx] {
                    candidates.push(Some(prev));
                }

                // Compute scores
                let scores: Vec<f64> = candidates
                    .iter()
                    .map(|c| score_pair(mention, *c, &weights))
                    .collect();

                // Softmax loss (negative log probability of gold)
                let probs = softmax(&scores);
                let gold_idx = match gold_ant {
                    None => 0,                    // NEW cluster is index 0
                    Some(ant_idx) => ant_idx + 1, // +1 because NEW is first
                };

                if gold_idx < probs.len() {
                    loss -= probs[gold_idx].ln().max(-10.0);
                }

                // Gradient update (simplified: just update for this mention)
                // Expected features - gold features
                for (c_idx, candidate) in candidates.iter().enumerate() {
                    let p = probs[c_idx];
                    let is_gold = c_idx == gold_idx;
                    let diff = if is_gold { p - 1.0 } else { p };

                    // Feature gradients
                    if candidate.is_none() {
                        update_weight(
                            &mut weights.new_cluster_bias,
                            &mut grad_squared.new_cluster_bias,
                            diff,
                            config.learning_rate,
                        );
                    } else {
                        let ant = candidate.expect("candidate.is_none() checked above");
                        let distance = mention.idx - ant.idx;

                        update_weight(
                            &mut weights.distance_decay,
                            &mut grad_squared.distance_decay,
                            diff * (distance as f64).ln_1p(),
                            config.learning_rate,
                        );

                        if mention.head_word == ant.head_word {
                            update_weight(
                                &mut weights.string_match,
                                &mut grad_squared.string_match,
                                diff,
                                config.learning_rate,
                            );
                        }

                        match (&mention.entity_type, &ant.entity_type) {
                            (Some(t1), Some(t2)) if t1 == t2 => {
                                update_weight(
                                    &mut weights.type_match,
                                    &mut grad_squared.type_match,
                                    diff,
                                    config.learning_rate,
                                );
                            }
                            (Some(_), Some(_)) => {
                                update_weight(
                                    &mut weights.type_mismatch,
                                    &mut grad_squared.type_mismatch,
                                    diff,
                                    config.learning_rate,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }

            total_loss += loss;
        }

        let avg_loss = total_loss / examples.len() as f64;
        if epoch % 2 == 0 || epoch == config.epochs - 1 {
            println!("Epoch {:2}: loss = {:.4}", epoch, avg_loss);
        }
    }

    weights
}

/// AdaGrad weight update.
fn update_weight(weight: &mut f64, grad_squared: &mut f64, grad: f64, lr: f64) {
    *grad_squared += grad * grad;
    *weight -= lr * grad / (*grad_squared + 1e-8).sqrt();
}

fn main() {
    println!("Joint Model Training from Coreference Data");
    println!("==========================================\n");

    // Create sample documents (simulating loaded coref data)
    let documents = [
        SimpleDocument {
            text: "Barack Obama visited France. Obama met with Macron. He praised the alliance."
                .to_string(),
            chains: vec![
                SimpleChain {
                    mentions: vec![
                        SimpleMention {
                            text: "Barack Obama".to_string(),
                            start: 0,
                            end: 12,
                        },
                        SimpleMention {
                            text: "Obama".to_string(),
                            start: 29,
                            end: 34,
                        },
                        SimpleMention {
                            text: "He".to_string(),
                            start: 52,
                            end: 54,
                        },
                    ],
                },
                SimpleChain {
                    mentions: vec![SimpleMention {
                        text: "France".to_string(),
                        start: 21,
                        end: 27,
                    }],
                },
                SimpleChain {
                    mentions: vec![SimpleMention {
                        text: "Macron".to_string(),
                        start: 44,
                        end: 50,
                    }],
                },
            ],
        },
        SimpleDocument {
            text: "Microsoft announced a new product. The company said sales increased."
                .to_string(),
            chains: vec![SimpleChain {
                mentions: vec![
                    SimpleMention {
                        text: "Microsoft".to_string(),
                        start: 0,
                        end: 9,
                    },
                    SimpleMention {
                        text: "The company".to_string(),
                        start: 35,
                        end: 46,
                    },
                ],
            }],
        },
        SimpleDocument {
            text: "Marie Curie discovered radium. She received two Nobel Prizes.".to_string(),
            chains: vec![SimpleChain {
                mentions: vec![
                    SimpleMention {
                        text: "Marie Curie".to_string(),
                        start: 0,
                        end: 11,
                    },
                    SimpleMention {
                        text: "She".to_string(),
                        start: 31,
                        end: 34,
                    },
                ],
            }],
        },
    ];

    // Convert to training examples
    let examples: Vec<TrainingExample> =
        documents.iter().map(document_to_training_example).collect();

    println!(
        "Converted {} documents to training examples\n",
        examples.len()
    );

    // Train the model
    let config = TrainingConfig {
        learning_rate: 0.1,
        epochs: 20,
        l1_reg: 0.001,
    };

    let weights = train(&examples, &config);

    println!("\nLearned weights:");
    println!("  new_cluster_bias: {:.4}", weights.new_cluster_bias);
    println!("  distance_decay:   {:.4}", weights.distance_decay);
    println!("  string_match:     {:.4}", weights.string_match);
    println!("  type_match:       {:.4}", weights.type_match);
    println!("  type_mismatch:    {:.4}", weights.type_mismatch);

    // Validate learned patterns
    println!("\nValidation:");
    println!(
        "  string_match > 0: {} (expected: PASS)",
        if weights.string_match > 0.0 {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!(
        "  distance_decay < 0: {} (expected: PASS)",
        if weights.distance_decay < 0.0 {
            "PASS"
        } else {
            "FAIL"
        }
    );
}
