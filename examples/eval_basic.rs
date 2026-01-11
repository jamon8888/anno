//! Basic evaluation example - evaluating Pattern and Statistical NER backends.
//!
//! Run with: cargo run --example eval_basic --features "eval-advanced"
//!
//! This example shows:
//! - How to use the EvalSystem to test NER backends
//! - How to access evaluation results

#[cfg(feature = "eval-advanced")]
use anno::eval::unified_evaluator::EvalSystem;
#[cfg(feature = "eval-advanced")]
use anno::eval::task_mapping::Task;
#[cfg(feature = "eval-advanced")]
use anno::eval::backend_name::BackendName;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══════════════════════════════════════════════════════════════════");
    println!("                     NER Backend Evaluation");
    println!("═══════════════════════════════════════════════════════════════════\n");

    #[cfg(not(feature = "eval-advanced"))]
    {
        println!("This example requires the 'eval-advanced' feature.");
        println!("Run with: cargo run --example eval_basic --features \"eval-advanced\"");
        return Ok(());
    }

    #[cfg(feature = "eval-advanced")]
    {
    // =========================================================================
    // 1. Quick evaluation on a subset of data
    // =========================================================================
    println!("1. Quick evaluation (50 examples)...\n");

        let results = EvalSystem::new()
            .with_tasks(vec![Task::NER])
            // Uses all suitable datasets by default if none specified
            // For synthetic data, we might need a specific dataset ID or task configuration
            // Currently EvalSystem runs on downloaded datasets by default
            // To maintain parity with old example, we'd need synthetic data support in TaskEvaluator
            // For now, let's run on a small sample of whatever is available
            .with_max_examples(Some(50))
            .add_backend_name(BackendName::RegexNER)
            .add_backend_name(BackendName::HeuristicNER)
            .add_backend_name(BackendName::StackedNER)
            .run()?;

        if let Some(std_results) = results.standard {
    // Print overall results
    println!("┌─────────────────┬───────────┬────────┬────────┐");
    println!("│ Backend         │ Precision │ Recall │ F1     │");
    println!("├─────────────────┼───────────┼────────┼────────┤");
            for (name, metrics) in &std_results.per_backend {
        println!(
            "│ {:15} │ {:7.1}%  │ {:6.1}% │ {:6.1}% │",
                    name,
                    metrics.precision * 100.0,
                    metrics.recall * 100.0,
                    metrics.f1 * 100.0,
        );
    }
    println!("└─────────────────┴───────────┴────────┴────────┘\n");
        } else {
            println!("No standard results produced.");
    }

    println!("\n═══════════════════════════════════════════════════════════════════");
    println!("                        Evaluation Complete");
    println!("═══════════════════════════════════════════════════════════════════\n");
    }

    Ok(())
}
