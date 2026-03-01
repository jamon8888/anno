//! Demonstrates coreference scoring with a toy scenario.
//!
//! Run: `cargo run -p anno-metrics --example scoring_demo`

use anno_metrics::coref::{CorefChain, Mention};
use anno_metrics::coref_metrics::{b_cubed_score, ceaf_e_score, conll_f1, muc_score, CorefScores};

fn main() {
    // Gold: three entities
    let gold = vec![
        CorefChain::new(vec![
            Mention::new("Alice", 0, 5),
            Mention::new("she", 20, 23),
            Mention::new("her", 40, 43),
        ]),
        CorefChain::new(vec![Mention::new("Bob", 6, 9), Mention::new("he", 30, 32)]),
        CorefChain::new(vec![
            Mention::new("the cat", 50, 57),
            Mention::new("it", 60, 62),
        ]),
    ];

    // Predicted: Alice is split into two clusters; Bob is correct; cat missed
    let pred = vec![
        CorefChain::new(vec![
            Mention::new("Alice", 0, 5),
            Mention::new("she", 20, 23),
        ]),
        CorefChain::new(vec![Mention::new("her", 40, 43)]),
        CorefChain::new(vec![Mention::new("Bob", 6, 9), Mention::new("he", 30, 32)]),
        CorefChain::new(vec![
            Mention::new("the cat", 50, 57),
            Mention::new("it", 60, 62),
        ]),
    ];

    let metrics: Vec<(&str, CorefScores)> = vec![
        ("MUC", CorefScores::from_tuple(muc_score(&pred, &gold))),
        ("B3", CorefScores::from_tuple(b_cubed_score(&pred, &gold))),
        (
            "CEAF-e",
            CorefScores::from_tuple(ceaf_e_score(&pred, &gold)),
        ),
    ];

    println!("Metric     P       R       F1");
    println!("------     -----   -----   -----");
    for (name, s) in &metrics {
        println!(
            "{:<10} {:.3}   {:.3}   {:.3}",
            name, s.precision, s.recall, s.f1
        );
    }
    println!("------");
    println!("CoNLL F1:  {:.3}", conll_f1(&pred, &gold));
}
