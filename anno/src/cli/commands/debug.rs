//! Debug command - Generate HTML debug visualization

use clap::Parser;
use std::fs;

use super::super::output::{color, print_signals};
use super::super::parser::ModelBackend;
use super::super::utils::{get_input_text, link_tracks_to_kb, resolve_coreference};

use crate::grounded::{render_document_html, GroundedDocument, Location, Signal, SignalId}; // Re-exported from anno-core
#[cfg(feature = "eval-advanced")]
use crate::ingest::url_resolver::{CompositeResolver, UrlResolver};
use crate::ingest::DocumentPreprocessor;

/// Generate HTML debug visualization
#[derive(Parser, Debug)]
pub struct DebugArgs {
    /// Input text to process
    #[arg(short, long)]
    pub text: Option<String>,

    /// Read input from file
    #[arg(short, long, value_name = "PATH")]
    pub file: Option<String>,

    /// Positional text arguments (alternative to --text)
    #[arg(value_name = "TEXT")]
    pub positional: Vec<String>,

    /// URL to fetch content from (requires eval-advanced feature)
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// Clean whitespace (normalize spaces, line breaks)
    #[arg(long)]
    pub clean: bool,

    /// Normalize Unicode (basic normalization)
    #[arg(long)]
    pub normalize: bool,

    /// Detect and record language
    #[arg(long)]
    pub detect_lang: bool,

    /// Export to graph format (neo4j, networkx, jsonld)
    #[arg(long, value_name = "FORMAT")]
    pub export_graph: Option<String>,

    /// Model backend to use
    #[arg(short, long, default_value = "stacked")]
    pub model: ModelBackend,

    /// Output as HTML (default: text)
    #[arg(long)]
    pub html: bool,

    /// Export GroundedDocument JSON to file (for pipeline integration)
    #[arg(long, value_name = "PATH")]
    pub export: Option<String>,

    /// Export format when using --export (full, signals, minimal)
    #[arg(long, default_value = "full", value_name = "FORMAT")]
    pub export_format: String,

    /// Write output to file (default: stdout)
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<String>,

    /// Run coreference resolution to form tracks
    #[arg(long)]
    pub coref: bool,

    /// Link tracks to KB identities (creates placeholder Wikidata IDs)
    #[arg(long)]
    pub link_kb: bool,

    /// Suppress status messages
    #[arg(short, long)]
    pub quiet: bool,

    /// Verbose output (repeat for more detail: -v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

/// Execute the debug command.
pub fn run(args: DebugArgs) -> Result<(), String> {
    // Level 1 + 2 (Signal → Track): Entity extraction + within-document coreference
    // With --link-kb: Level 1 + 2 + 3 (Signal → Track → Identity): Adds KB linking
    // This builds the full hierarchy that could be used by coalescing for better clustering

    // Resolve input: URL, file, text, or stdin
    let mut raw_text = if let Some(url) = &args.url {
        #[cfg(feature = "eval-advanced")]
        {
            let resolver = CompositeResolver::new();
            let resolved = resolver
                .resolve(url)
                .map_err(|e| format!("Failed to fetch URL {}: {}", url, e))?;
            resolved.text
        }
        #[cfg(not(feature = "eval-advanced"))]
        {
            #[allow(unused_variables)]
            let _url = url;
            return Err("URL resolution requires 'eval-advanced' feature. Enable with: cargo build --features eval-advanced".to_string());
        }
    } else {
        get_input_text(&args.text, args.file.as_deref(), &args.positional)?
    };

    // Preprocess text if requested
    if args.clean || args.normalize || args.detect_lang {
        let preprocessor = DocumentPreprocessor {
            clean_whitespace: args.clean,
            normalize_unicode: args.normalize,
            detect_language: args.detect_lang,
            chunk_size: None,
        };
        let prepared = preprocessor.prepare(&raw_text);
        raw_text = prepared.text;
        if args.verbose >= 1 && !prepared.metadata.is_empty() {
            eprintln!("Preprocessing metadata: {:?}", prepared.metadata);
        }
    }

    let text = raw_text;
    let model = args.model.create_model()?;

    let entities = model
        .extract_entities(&text, None)
        .map_err(|e| format!("Extraction failed: {}", e))?;

    // Build grounded document with validated signals
    // Always use actual offsets from model - don't re-find text (which would always find first occurrence)
    let mut doc = GroundedDocument::new("debug", &text);
    let mut signal_ids: Vec<SignalId> = Vec::new();

    for e in &entities {
        let signal = Signal::new(
            SignalId::ZERO,
            Location::text(e.start, e.end),
            &e.text,
            e.entity_type.as_label(),
            e.confidence as f32,
        );
        let id = doc.add_signal(signal);
        signal_ids.push(id);
    }

    // Run coreference resolution if requested
    if args.coref {
        resolve_coreference(&mut doc, &text, &signal_ids);
    }

    // Link tracks to KB identities if requested
    if args.link_kb {
        link_tracks_to_kb(&mut doc);
    }

    // Export to file if requested
    if let Some(export_path) = args.export {
        let export_data = match args.export_format.as_str() {
            "full" => serde_json::to_value(&doc)
                .map_err(|e| format!("Failed to serialize GroundedDocument: {}", e))?,
            "signals" => {
                let signals: Vec<_> = doc.signals().to_vec();
                serde_json::json!({
                    "id": doc.id,
                    "text": doc.text,
                    "signals": signals
                })
            }
            "minimal" => {
                let signals: Vec<_> = doc
                    .signals()
                    .iter()
                    .map(|s| {
                        let (start, end) = s.text_offsets().unwrap_or((0, 0));
                        serde_json::json!({
                            "surface": s.surface(),
                            "label": s.label(),
                            "start": start,
                            "end": end,
                            "confidence": s.confidence
                        })
                    })
                    .collect();
                serde_json::json!({
                    "id": doc.id,
                    "text": doc.text,
                    "signals": signals
                })
            }
            _ => {
                return Err(format!(
                    "Invalid export format '{}'. Use: full, signals, or minimal",
                    args.export_format
                ));
            }
        };

        let json = serde_json::to_string_pretty(&export_data)
            .map_err(|e| format!("Failed to serialize export data: {}", e))?;

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&export_path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "Failed to create directory for export file '{}': {}",
                        export_path, e
                    )
                })?;
            }
        }

        fs::write(&export_path, json)
            .map_err(|e| format!("Failed to write export file '{}': {}", export_path, e))?;
        if !args.quiet {
            eprintln!(
                "{} Exported {} format to {}",
                color("32", "✓"),
                args.export_format,
                export_path
            );
        }
    }

    // Build spatial index and validate
    let _index = doc.build_text_index();
    let errors = doc.validate();

    if !errors.is_empty() && !args.quiet {
        eprintln!(
            "{} {} validation errors:",
            color("33", "warning:"),
            errors.len()
        );
        for e in &errors {
            eprintln!("  - {}", e);
        }
    }

    // Output format
    if args.html
        || args
            .output
            .as_ref()
            .map(|p| p.ends_with(".html"))
            .unwrap_or(false)
    {
        // Generate HTML
        let html = render_document_html(&doc);

        if let Some(path) = &args.output {
            fs::write(path, &html).map_err(|e| format!("Failed to write {}: {}", path, e))?;
            if !args.quiet {
                println!("{} HTML written to: {}", color("32", "ok:"), path);
            }
        } else {
            println!("{}", html);
        }
    } else {
        // Text output (default) - use dense format with verbose levels
        if doc.signals().is_empty() {
            println!("(no entities)");
        } else {
            // Use verbose level from args, but ensure tracks/identities are shown if coref/link_kb was run
            let effective_verbose = if args.coref || args.link_kb {
                args.verbose.max(2) // At least level 2 if coref or KB linking was run
            } else {
                args.verbose
            };
            print_signals(&doc, &text, effective_verbose);
        }
    }

    Ok(())
}
