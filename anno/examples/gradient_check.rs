//! Numerical gradient checking for joint model.
//!
//! Verifies that analytical gradients match numerical gradients
//! (finite differences), ensuring correctness of the training loop.
//!
//! Run with: cargo run -p anno --example gradient_check

use std::collections::HashMap;

// ===========================================================================
// Types (minimal standalone version)
// ===========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum EntityType {
    Person,
    Organization,
    #[allow(dead_code)]
    Location,
}

impl EntityType {
    #[allow(dead_code)]
    fn as_label(&self) -> &str {
        match self {
            EntityType::Person => "PER",
            EntityType::Organization => "ORG",
            EntityType::Location => "LOC",
        }
    }
}

#[derive(Debug, Clone)]
struct JointMention {
    #[allow(dead_code)]
    idx: usize,
    text: String,
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
struct TrainingExample {
    mentions: Vec<JointMention>,
    #[allow(dead_code)]
    gold_ner: HashMap<usize, EntityType>,
    #[allow(dead_code)]
    gold_coref: HashMap<usize, Option<usize>>,
}

// ===========================================================================
// Score Computation
// ===========================================================================

fn compute_score(
    weights: &JointWeights,
    example: &TrainingExample,
    ner: &HashMap<usize, EntityType>,
    coref: &HashMap<usize, Option<usize>>,
) -> f64 {
    let mut score = 0.0;

    // Unary coref scores
    for (idx, ante) in coref {
        if ante.is_none() {
            score += weights.new_cluster_bias;
        } else if let Some(ante_idx) = ante {
            let dist = (*idx as f64 - *ante_idx as f64).abs();
            score -= weights.distance_decay * (dist + 1.0).ln();

            if *idx < example.mentions.len() && *ante_idx < example.mentions.len() {
                let m_i = &example.mentions[*idx];
                let m_j = &example.mentions[*ante_idx];
                if m_i.text.to_lowercase() == m_j.text.to_lowercase() {
                    score += weights.string_match;
                }
            }
        }
    }

    // Pairwise coref-NER scores
    for (idx, ante) in coref {
        if let Some(ante_idx) = ante {
            if let (Some(type_i), Some(type_j)) = (ner.get(idx), ner.get(ante_idx)) {
                if type_i == type_j {
                    score += weights.type_match;
                } else {
                    score += weights.type_mismatch;
                }
            }
        }
    }

    score
}

// ===========================================================================
// Analytical Gradient
// ===========================================================================

#[derive(Debug, Clone, Default)]
struct Gradients {
    new_cluster_bias: f64,
    distance_decay: f64,
    string_match: f64,
    type_match: f64,
    type_mismatch: f64,
}

fn compute_analytical_gradients(
    example: &TrainingExample,
    ner: &HashMap<usize, EntityType>,
    coref: &HashMap<usize, Option<usize>>,
) -> Gradients {
    let mut grads = Gradients::default();

    // Gradient of score w.r.t. each weight parameter
    for (idx, ante) in coref {
        if ante.is_none() {
            grads.new_cluster_bias += 1.0;
        } else if let Some(ante_idx) = ante {
            let dist = (*idx as f64 - *ante_idx as f64).abs();
            grads.distance_decay -= (dist + 1.0).ln();

            if *idx < example.mentions.len() && *ante_idx < example.mentions.len() {
                let m_i = &example.mentions[*idx];
                let m_j = &example.mentions[*ante_idx];
                if m_i.text.to_lowercase() == m_j.text.to_lowercase() {
                    grads.string_match += 1.0;
                }
            }
        }
    }

    for (idx, ante) in coref {
        if let Some(ante_idx) = ante {
            if let (Some(type_i), Some(type_j)) = (ner.get(idx), ner.get(ante_idx)) {
                if type_i == type_j {
                    grads.type_match += 1.0;
                } else {
                    grads.type_mismatch += 1.0;
                }
            }
        }
    }

    grads
}

// ===========================================================================
// Numerical Gradient (finite differences)
// ===========================================================================

fn compute_numerical_gradients(
    weights: &JointWeights,
    example: &TrainingExample,
    ner: &HashMap<usize, EntityType>,
    coref: &HashMap<usize, Option<usize>>,
    epsilon: f64,
) -> Gradients {
    let mut grads = Gradients::default();

    // Numerical gradient for each parameter

    // new_cluster_bias
    {
        let mut w_plus = weights.clone();
        let mut w_minus = weights.clone();
        w_plus.new_cluster_bias += epsilon;
        w_minus.new_cluster_bias -= epsilon;
        let s_plus = compute_score(&w_plus, example, ner, coref);
        let s_minus = compute_score(&w_minus, example, ner, coref);
        grads.new_cluster_bias = (s_plus - s_minus) / (2.0 * epsilon);
    }

    // distance_decay
    {
        let mut w_plus = weights.clone();
        let mut w_minus = weights.clone();
        w_plus.distance_decay += epsilon;
        w_minus.distance_decay -= epsilon;
        let s_plus = compute_score(&w_plus, example, ner, coref);
        let s_minus = compute_score(&w_minus, example, ner, coref);
        grads.distance_decay = (s_plus - s_minus) / (2.0 * epsilon);
    }

    // string_match
    {
        let mut w_plus = weights.clone();
        let mut w_minus = weights.clone();
        w_plus.string_match += epsilon;
        w_minus.string_match -= epsilon;
        let s_plus = compute_score(&w_plus, example, ner, coref);
        let s_minus = compute_score(&w_minus, example, ner, coref);
        grads.string_match = (s_plus - s_minus) / (2.0 * epsilon);
    }

    // type_match
    {
        let mut w_plus = weights.clone();
        let mut w_minus = weights.clone();
        w_plus.type_match += epsilon;
        w_minus.type_match -= epsilon;
        let s_plus = compute_score(&w_plus, example, ner, coref);
        let s_minus = compute_score(&w_minus, example, ner, coref);
        grads.type_match = (s_plus - s_minus) / (2.0 * epsilon);
    }

    // type_mismatch
    {
        let mut w_plus = weights.clone();
        let mut w_minus = weights.clone();
        w_plus.type_mismatch += epsilon;
        w_minus.type_mismatch -= epsilon;
        let s_plus = compute_score(&w_plus, example, ner, coref);
        let s_minus = compute_score(&w_minus, example, ner, coref);
        grads.type_mismatch = (s_plus - s_minus) / (2.0 * epsilon);
    }

    grads
}

// ===========================================================================
// Gradient Check
// ===========================================================================

fn check_gradients(
    analytical: &Gradients,
    numerical: &Gradients,
    tolerance: f64,
) -> Vec<(String, bool, f64)> {
    let mut results = Vec::new();

    let check = |name: &str, a: f64, n: f64| {
        let diff = (a - n).abs();
        let rel_diff = if n.abs() > 1e-8 { diff / n.abs() } else { diff };
        let pass = rel_diff < tolerance;
        (name.to_string(), pass, rel_diff)
    };

    results.push(check(
        "new_cluster_bias",
        analytical.new_cluster_bias,
        numerical.new_cluster_bias,
    ));
    results.push(check(
        "distance_decay",
        analytical.distance_decay,
        numerical.distance_decay,
    ));
    results.push(check(
        "string_match",
        analytical.string_match,
        numerical.string_match,
    ));
    results.push(check(
        "type_match",
        analytical.type_match,
        numerical.type_match,
    ));
    results.push(check(
        "type_mismatch",
        analytical.type_mismatch,
        numerical.type_mismatch,
    ));

    results
}

// ===========================================================================
// Test Cases
// ===========================================================================

fn create_test_cases() -> Vec<(
    String,
    TrainingExample,
    HashMap<usize, EntityType>,
    HashMap<usize, Option<usize>>,
)> {
    let mut cases = Vec::new();

    // Case 1: Simple new cluster
    {
        let mentions = vec![JointMention {
            idx: 0,
            text: "Alice".to_string(),
        }];
        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Person);
        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None); // New cluster
        let example = TrainingExample {
            mentions,
            gold_ner: gold_ner.clone(),
            gold_coref: gold_coref.clone(),
        };
        cases.push(("new_cluster".to_string(), example, gold_ner, gold_coref));
    }

    // Case 2: String match coreference
    {
        let mentions = vec![
            JointMention {
                idx: 0,
                text: "Obama".to_string(),
            },
            JointMention {
                idx: 1,
                text: "Obama".to_string(),
            },
        ];
        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Person);
        gold_ner.insert(1, EntityType::Person);
        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, Some(0));
        let example = TrainingExample {
            mentions,
            gold_ner: gold_ner.clone(),
            gold_coref: gold_coref.clone(),
        };
        cases.push(("string_match".to_string(), example, gold_ner, gold_coref));
    }

    // Case 3: Type match coreference
    {
        let mentions = vec![
            JointMention {
                idx: 0,
                text: "Alice".to_string(),
            },
            JointMention {
                idx: 1,
                text: "she".to_string(),
            },
        ];
        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Person);
        gold_ner.insert(1, EntityType::Person);
        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, Some(0));
        let example = TrainingExample {
            mentions,
            gold_ner: gold_ner.clone(),
            gold_coref: gold_coref.clone(),
        };
        cases.push(("type_match".to_string(), example, gold_ner, gold_coref));
    }

    // Case 4: Type mismatch (negative case)
    {
        let mentions = vec![
            JointMention {
                idx: 0,
                text: "Apple".to_string(),
            },
            JointMention {
                idx: 1,
                text: "it".to_string(),
            },
        ];
        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Organization);
        gold_ner.insert(1, EntityType::Person); // Wrong type
        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, Some(0));
        let example = TrainingExample {
            mentions,
            gold_ner: gold_ner.clone(),
            gold_coref: gold_coref.clone(),
        };
        cases.push(("type_mismatch".to_string(), example, gold_ner, gold_coref));
    }

    // Case 5: Long distance coreference
    {
        let mentions = vec![
            JointMention {
                idx: 0,
                text: "Alice".to_string(),
            },
            JointMention {
                idx: 1,
                text: "Bob".to_string(),
            },
            JointMention {
                idx: 2,
                text: "Charlie".to_string(),
            },
            JointMention {
                idx: 3,
                text: "she".to_string(),
            }, // Links back to Alice
        ];
        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Person);
        gold_ner.insert(1, EntityType::Person);
        gold_ner.insert(2, EntityType::Person);
        gold_ner.insert(3, EntityType::Person);
        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, None);
        gold_coref.insert(2, None);
        gold_coref.insert(3, Some(0)); // Distance = 3
        let example = TrainingExample {
            mentions,
            gold_ner: gold_ner.clone(),
            gold_coref: gold_coref.clone(),
        };
        cases.push(("long_distance".to_string(), example, gold_ner, gold_coref));
    }

    cases
}

// ===========================================================================
// Main
// ===========================================================================

fn main() {
    println!("Gradient Check for Joint Model");
    println!("===============================\n");

    let weights = JointWeights {
        new_cluster_bias: 0.1,
        distance_decay: 0.05,
        string_match: 0.3,
        type_match: 0.2,
        type_mismatch: -0.1,
    };

    let epsilon = 1e-5;
    let tolerance = 1e-3;

    let test_cases = create_test_cases();
    let mut all_passed = true;

    for (name, example, ner, coref) in test_cases {
        println!("Test case: {}", name);
        println!("-----------");

        let analytical = compute_analytical_gradients(&example, &ner, &coref);
        let numerical = compute_numerical_gradients(&weights, &example, &ner, &coref, epsilon);

        println!("Analytical gradients:");
        println!("  new_cluster_bias: {:.6}", analytical.new_cluster_bias);
        println!("  distance_decay:   {:.6}", analytical.distance_decay);
        println!("  string_match:     {:.6}", analytical.string_match);
        println!("  type_match:       {:.6}", analytical.type_match);
        println!("  type_mismatch:    {:.6}", analytical.type_mismatch);

        println!("Numerical gradients:");
        println!("  new_cluster_bias: {:.6}", numerical.new_cluster_bias);
        println!("  distance_decay:   {:.6}", numerical.distance_decay);
        println!("  string_match:     {:.6}", numerical.string_match);
        println!("  type_match:       {:.6}", numerical.type_match);
        println!("  type_mismatch:    {:.6}", numerical.type_mismatch);

        let results = check_gradients(&analytical, &numerical, tolerance);
        println!("Gradient check results:");
        for (param, pass, rel_diff) in &results {
            let status = if *pass { "PASS" } else { "FAIL" };
            println!("  {}: {} (rel_diff: {:.6})", param, status, rel_diff);
            if !pass {
                all_passed = false;
            }
        }
        println!();
    }

    println!("================================");
    if all_passed {
        println!("All gradient checks PASSED!");
    } else {
        println!("Some gradient checks FAILED!");
    }
}
