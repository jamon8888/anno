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

use std::io;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, ValueEnum};

use anno::Model;

#[cfg(not(any(feature = "eval", feature = "eval-advanced")))]
use anno::{AutoNER, HeuristicNER, RegexNER};

#[cfg(feature = "onnx")]
// GLiNER exports available when onnx feature is enabled
#[allow(unused_imports)]
use anno::{DEFAULT_GLINER2_MODEL, DEFAULT_GLINER_MODEL};

// ============================================================================
// CLI Structure
// ============================================================================

/// Information Extraction CLI - NER, Coreference, Relations, Entity Linking
///
/// UX/DESIGN NOTES:
/// - See hack/CLI_UX_CRITIQUE.md for comprehensive UX analysis
/// - Key issues: inconsistent input methods, model discoverability, output format handling
/// - TODO: Standardize input patterns, add `anno models` command, improve error messages

// ============================================================================
// Shared Types (Legacy - most moved to cli module)
// ============================================================================
// ModelBackend, OutputFormat, EvalTask are now in src/cli/parser.rs

/// Model backend selection (legacy - use anno::cli::parser::ModelBackend)
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
#[allow(dead_code)]
enum ModelBackend {
    /// Regex matching only (dates, emails, etc.)
    Pattern,
    /// Heuristic NER (persons, orgs, locs via capitalization + context)
    #[value(alias = "statistical")]
    Heuristic,
    /// Minimal heuristic (low complexity experiment)
    Minimal,
    /// Automatic (Language-detected routing)
    Auto,
    /// Stacked: Pattern + Heuristic (default)
    #[default]
    Stacked,
    /// GLiNER via ONNX (requires --features onnx)
    #[cfg(feature = "onnx")]
    Gliner,
    /// GLiNER2 multi-task (NER + classification + structure, requires --features onnx)
    #[cfg(feature = "onnx")]
    Gliner2,
    /// NuNER (requires --features onnx)
    #[cfg(feature = "onnx")]
    Nuner,
    /// W2NER for nested entities (requires --features onnx)
    #[cfg(feature = "onnx")]
    W2ner,
    /// GLiNER via Candle (requires --features candle)
    #[cfg(feature = "candle")]
    GlinerCandle,
}

impl ModelBackend {
    #[allow(dead_code)] // Used in tests
    fn create_model(self) -> Result<Box<dyn Model>, String> {
        // Use BackendFactory for consistent backend creation when available
        #[cfg(any(feature = "eval", feature = "eval-advanced"))]
        {
            // Map backend enum to factory name
            let factory_name = match self {
                Self::Pattern => "pattern",
                Self::Heuristic => "heuristic",
                Self::Minimal => "heuristic", // Minimal uses heuristic
                Self::Auto => "stacked",      // Auto uses stacked
                Self::Stacked => "stacked",
                #[cfg(feature = "onnx")]
                Self::Gliner => "gliner_onnx",
                #[cfg(feature = "onnx")]
                Self::Gliner2 => "gliner2",
                #[cfg(feature = "onnx")]
                Self::Nuner => "nuner",
                #[cfg(feature = "onnx")]
                Self::W2ner => "w2ner",
                #[cfg(feature = "candle")]
                Self::GlinerCandle => "gliner_candle",
            };
            return anno::eval::backend_factory::BackendFactory::create(factory_name)
                .map_err(|e| format!("Failed to create model '{}': {}", self.name(), e));
        }
        // Fallback to original implementation when eval feature not available
        #[cfg(not(any(feature = "eval", feature = "eval-advanced")))]
        match self {
            Self::Pattern => Ok(Box::new(RegexNER::new())),
            Self::Heuristic => Ok(Box::new(HeuristicNER::new())),
            // Minimal was merged into HeuristicNER
            Self::Minimal => Ok(Box::new(HeuristicNER::new())),
            Self::Auto => {
                // AutoNER just routes to default (StackedNER), doesn't combine models
                Ok(Box::new(AutoNER::new()))
            }
            Self::Stacked => Ok(Box::new(StackedNER::default())),
            #[cfg(feature = "onnx")]
            Self::Gliner => anno::GLiNEROnnx::new(anno::DEFAULT_GLINER_MODEL)
                .map(|m| Box::new(m) as Box<dyn Model>)
                .map_err(|e| format!("Failed to load GLiNER: {}\n  Tip: Use 'anno models info gliner' to check model status.", e)),
            #[cfg(feature = "onnx")]
            Self::Gliner2 => anno::backends::gliner2::GLiNER2Onnx::from_pretrained(anno::DEFAULT_GLINER2_MODEL)
                .map(|m| Box::new(m) as Box<dyn Model>)
                .map_err(|e| format!("Failed to load GLiNER2: {}\n  Tip: Use 'anno models info gliner2' to check model status.", e)),
            #[cfg(feature = "onnx")]
            Self::Nuner => Err("NuNER not yet implemented in CLI.\n  Tip: Use 'anno models list' to see available models.".to_string()),
            #[cfg(feature = "onnx")]
            Self::W2ner => Err("W2NER not yet implemented in CLI.\n  Tip: Use 'anno models list' to see available models.".to_string()),
            #[cfg(feature = "candle")]
            Self::GlinerCandle => Err("GLiNER Candle not yet implemented in CLI.\n  Tip: Use 'anno models list' to see available models.".to_string()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::Heuristic => "heuristic",
            Self::Minimal => "minimal",
            Self::Auto => "auto",
            Self::Stacked => "stacked",
            #[cfg(feature = "onnx")]
            Self::Gliner => "gliner",
            #[cfg(feature = "onnx")]
            Self::Gliner2 => "gliner2",
            #[cfg(feature = "onnx")]
            Self::Nuner => "nuner",
            #[cfg(feature = "onnx")]
            Self::W2ner => "w2ner",
            #[cfg(feature = "candle")]
            Self::GlinerCandle => "gliner-candle",
        }
    }
}

// OutputFormat and EvalTask are now in src/cli/parser.rs

// ============================================================================
// Command Arguments
// ============================================================================

// All Args structs moved to cli module - see src/cli/commands/*.rs

// ============================================================================
// Legacy Command Handlers (to be extracted)
// ============================================================================
// All Args structs are now in src/cli/commands/*.rs - these are just the handlers

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> ExitCode {
    // Initialize tracing subscriber when instrument feature is enabled
    #[cfg(feature = "instrument")]
    {
        use tracing_subscriber::{fmt, EnvFilter};
        // Default to info level, can be overridden via RUST_LOG env var
        // Example: RUST_LOG=anno=debug,anno::backends=trace
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

// ============================================================================
// Helper Functions
// ============================================================================

// find_similar_models removed - was only used in dead cmd_models function

// ============================================================================
// Commands
// ============================================================================
// NOTE: All cmd_* functions have been removed. The CLI now uses the module-based
// command handlers (extract::run, debug::run, etc.) defined in anno/src/cli/commands/

// Legacy cmd_* functions removed - they were never called after refactoring to module-based commands

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::ModelBackend;
    use anno::cli::output::{color, confidence_bar, metric_colored, type_color};
    use anno::cli::utils::{
        detect_quantifier, is_likely_female, is_likely_male, is_negated, normalize_entity_name,
        parse_gold_spec,
    };
    use anno_core::Quantifier;

    #[test]
    fn test_parse_gold_spec_simple() {
        let spec =
            parse_gold_spec("Marie Curie:PER:0:11").expect("Test gold spec should parse correctly");
        assert_eq!(spec.text, "Marie Curie");
        assert_eq!(spec.label, "PER");
        assert_eq!(spec.start, 0);
        assert_eq!(spec.end, 11);
    }

    #[test]
    fn test_parse_gold_spec_with_colon_in_text() {
        // URL containing colons
        let spec = parse_gold_spec("https://example.com:URL:0:19")
            .expect("Test gold spec should parse correctly");
        assert_eq!(spec.text, "https://example.com");
        assert_eq!(spec.label, "URL");
        assert_eq!(spec.start, 0);
        assert_eq!(spec.end, 19);
    }

    #[test]
    fn test_parse_gold_spec_invalid() {
        assert!(parse_gold_spec("invalid").is_none());
        assert!(parse_gold_spec("text:label").is_none());
        assert!(parse_gold_spec("text:label:notanumber:10").is_none());
    }

    #[test]
    fn test_is_negated() {
        assert!(is_negated("He is not a doctor", 10));
        assert!(is_negated("Never trust John", 12));
        assert!(!is_negated("Trust John", 6));
    }

    #[test]
    fn test_detect_quantifier() {
        assert_eq!(
            detect_quantifier("every employee", 6),
            Some(Quantifier::Universal)
        );
        assert_eq!(
            detect_quantifier("some people", 5),
            Some(Quantifier::Existential)
        );
        assert_eq!(
            detect_quantifier("the manager", 4),
            Some(Quantifier::Definite)
        );
        assert_eq!(detect_quantifier("John Smith", 0), None);
    }

    #[test]
    fn test_model_backend_names() {
        assert_eq!(ModelBackend::Pattern.name(), "pattern");
        assert_eq!(ModelBackend::Heuristic.name(), "heuristic");
        assert_eq!(ModelBackend::Stacked.name(), "stacked");
    }

    #[test]
    fn test_confidence_bar_normal() {
        // Normal cases - function returns a visual bar, not percentage
        let bar = confidence_bar(0.5);
        assert!(bar.contains("#")); // 50% should have some filled chars
        assert!(bar.contains(".")); // and some empty chars

        let bar = confidence_bar(1.0);
        assert!(bar.contains("#")); // 100% should be fully filled

        let bar = confidence_bar(0.0);
        assert!(bar.contains(".")); // 0% should be mostly empty
    }

    #[test]
    fn test_confidence_bar_clamping() {
        // Edge case: confidence slightly over 1.0 should not panic
        // The bar is clamped to 10 filled chars max
        let bar = confidence_bar(1.01);
        assert!(bar.contains("#")); // Should have filled chars
                                    // Just verify it doesn't panic - bar is clamped internally

        // Edge case: confidence at exactly 1.0
        let bar = confidence_bar(1.0);
        assert!(bar.contains("#")); // 100% should be fully filled
    }

    #[test]
    fn test_is_negated_unicode() {
        // Test with Unicode text (character offsets, not byte offsets)
        // "café" has 4 chars but 5 bytes (é is 2 bytes in UTF-8)
        assert!(!is_negated("café John", 5)); // "John" starts at char 5
        assert!(is_negated("not café John", 9)); // "not" is in the prefix
    }

    #[test]
    fn test_detect_quantifier_unicode() {
        // Test with Unicode text
        // "every café employee" - "employee" starts at char index 11
        assert_eq!(
            detect_quantifier("every café employee", 11),
            None // "café" is not a quantifier
        );
        // "every employee" still works
        assert_eq!(
            detect_quantifier("every employee", 6),
            Some(Quantifier::Universal)
        );
    }

    #[test]
    fn test_normalize_entity_name() {
        assert_eq!(normalize_entity_name("  John Smith  "), "john smith");
        assert_eq!(normalize_entity_name("MARIE CURIE"), "marie curie");
        assert_eq!(normalize_entity_name("Test"), "test");
    }

    #[test]
    fn test_is_likely_male() {
        assert!(is_likely_male("John Smith"));
        assert!(is_likely_male("Barack Obama"));
        assert!(!is_likely_male("Marie Curie"));
        assert!(!is_likely_male("Unknown Person"));
    }

    #[test]
    fn test_is_likely_female() {
        assert!(is_likely_female("Marie Curie"));
        assert!(is_likely_female("Hillary Clinton"));
        assert!(!is_likely_female("John Smith"));
        assert!(!is_likely_female("Unknown Person"));
    }

    #[test]
    fn test_type_color() {
        assert_eq!(type_color("PER"), "1;34");
        assert_eq!(type_color("person"), "1;34");
        assert_eq!(type_color("ORG"), "1;32");
        assert_eq!(type_color("LOC"), "1;33");
        assert_eq!(type_color("UNKNOWN"), "1;37");
    }

    #[test]
    fn test_metric_colored() {
        // High score (>= 90)
        let result = metric_colored(95.0);
        assert!(result.contains("95.0"));

        // Medium score (>= 70)
        let result = metric_colored(75.0);
        assert!(result.contains("75.0"));

        // Low score (< 50)
        let result = metric_colored(30.0);
        assert!(result.contains("30.0"));
    }

    #[test]
    fn test_color_function() {
        // When not in a terminal, color() should return plain text
        // This test verifies the function doesn't panic
        let result = color("32", "test");
        assert!(result.contains("test"));
    }
}
