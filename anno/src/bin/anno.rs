//! anno - Information Extraction CLI
//!
//! A unified toolkit for named entity recognition, coreference resolution,
//! relation extraction, and entity linking.
//!
//! # Capabilities
//!
//! - **NER**: Named Entity Recognition (persons, organizations, locations, etc.)
//! - **Coreference**: Link mentions to the same entity ("She" → "Marie Curie")
//! - **Relations**: Extract (head, relation, tail) triples
//! - **Entity Linking**: Connect entities to knowledge bases (Wikidata)
//! - **Events**: Discourse-level event extraction
//!
//! # Signal → Track → Identity Hierarchy
//!
//! ```text
//! Level 1 (Signal)   : Raw detections with spans
//! Level 2 (Track)    : Within-document coreference chains
//! Level 3 (Identity) : Cross-document entity coalescing and KB linking
//! ```
//!
//! # Usage
//!
//! ```bash
//! # Basic NER extraction
//! anno extract "Marie Curie won the Nobel Prize."
//!
//! # Debug with coreference and KB linking
//! anno debug --coref --link-kb -t "Barack Obama met Angela Merkel. He praised her."
//!
//! # Evaluate against gold annotations
//! anno eval -t "..." -g "Marie Curie:PER:0:11"
//!
//! # Validate annotation files
//! anno validate file.jsonl
//!
//! # Show available models and features
//! anno info
//! ```
//!
//! All logic lives in `anno::cli::*`; this is just the dispatcher.

use std::io;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

fn main() -> ExitCode {
    // Initialize tracing subscriber when instrument feature is enabled
    #[cfg(feature = "instrument")]
    {
        use tracing_subscriber::{fmt, EnvFilter};
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
        fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(false)
            .init();
    }

    use anno::cli::commands::*;
    use anno::cli::output::color;
    use anno::cli::parser::{Cli, Commands, ModelBackend, OutputFormat};
    use clap_complete::generate;

    let cli = Cli::parse();

    let result: Result<(), String> = match cli.command {
        Some(Commands::Extract(args)) => extract::run(args),
        Some(Commands::Debug(args)) => debug::run(args),
        Some(Commands::Eval(args)) => eval::run(args),
        Some(Commands::Validate(args)) => validate::run(args),
        Some(Commands::Analyze(args)) => analyze::run(args),
        Some(Commands::Dataset(args)) => dataset::run(args),
        #[cfg(feature = "eval-advanced")]
        Some(Commands::Benchmark(args)) => benchmark::run(args),
        Some(Commands::Info) => info::run(),
        Some(Commands::Models(args)) => models::run(args),
        #[cfg(feature = "eval-advanced")]
        Some(Commands::CrossDoc(args)) => crossdoc::run(args),
        #[cfg(feature = "eval-advanced")]
        Some(Commands::Strata(args)) => strata::run(args),
        Some(Commands::Enhance(args)) => enhance::run(args),
        Some(Commands::Pipeline(args)) => pipeline::run(args),
        Some(Commands::Query(args)) => query::run(args),
        Some(Commands::Compare(args)) => compare::run(args),
        Some(Commands::Cache(args)) => cache::run(args),
        Some(Commands::Config(args)) => config::run(args),
        Some(Commands::Batch(args)) => batch::run(args),
        Some(Commands::Joint(args)) => joint::run(args),
        Some(Commands::Privacy(args)) => privacy::run(args),
        Some(Commands::Watch(args)) => watch::run(args),
        Some(Commands::Domain(args)) => domain::run(args),
        Some(Commands::Explain(args)) => explain::run(args),
        Some(Commands::Singleton(args)) => singleton::run(args),
        Some(Commands::Completions { shell }) => {
            generate(shell, &mut Cli::command(), "anno", &mut io::stdout());
            Ok(())
        }
        None => {
            // No subcommand: treat positional args as text to extract
            if cli.text.is_empty() {
                eprintln!("No input provided. Run `anno --help` for usage.");
                return ExitCode::FAILURE;
            }
            let text = cli.text.join(" ");
            extract::run(anno::cli::commands::ExtractArgs {
                url: None,
                clean: false,
                normalize: false,
                detect_lang: false,
                export_graph: None,
                text: Some(text),
                file: None,
                model: ModelBackend::default(),
                labels: vec![],
                format: OutputFormat::default(),
                export: None,
                export_format: "full".to_string(),
                negation: false,
                quantifiers: false,
                verbose: 0,
                quiet: false,
                positional: vec![],
            })
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", color("31", "error:"), e);
            ExitCode::FAILURE
        }
    }
}
