//! Batch command — multi-document extraction with optional parallelism and result caching.
//!
//! ## Parallelism (`--parallel N`)
//!
//! When `N > 1`, documents are processed concurrently using a Rayon thread pool capped at `N`
//! workers. The model is wrapped in an `Arc` and shared across threads; all anno backends
//! satisfy `Send + Sync`.
//!
//! ## Caching (`--cache`)
//!
//! Results are persisted to `{cache_dir}/results/{model}-{version}/{shard}/{hash}.json`.
//! The cache key is `xxh3_64(text)` — model name and version are encoded in the path so
//! changing backend or weights automatically misses. Cache entries are never evicted
//! automatically; use `anno cache clear` to flush.

use super::super::parser::{ModelBackend, OutputFormat};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Batch processing
#[derive(Parser, Debug)]
pub struct BatchArgs {
    /// Process directory of .txt/.md files
    #[arg(short, long, value_name = "DIR")]
    pub dir: Option<String>,

    /// Read from stdin (JSONL: one `{"id":"…","text":"…"}` object per line)
    #[arg(long)]
    pub stdin: bool,

    /// Model backend to use
    #[arg(short, long, default_value = "stacked")]
    pub model: ModelBackend,

    /// Run coreference resolution on each document
    #[arg(long)]
    pub coref: bool,

    /// Link tracks to KB identities
    #[arg(long)]
    pub link_kb: bool,

    /// Number of parallel workers (1 = sequential)
    #[arg(short, long, default_value = "1")]
    pub parallel: usize,

    /// Show progress bar
    #[arg(long)]
    pub progress: bool,

    /// Cache extraction results keyed by text hash + model version
    #[arg(long)]
    pub cache: bool,

    /// Output directory for results
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<String>,

    /// Output format
    #[arg(long, default_value = "grounded")]
    pub format: OutputFormat,

    /// Suppress status messages
    #[arg(short, long)]
    pub quiet: bool,
}


// Cache helpers


/// Derive a filesystem path for a cached document result.
///
/// Layout: `{cache_root}/results/{model}-{version}/{first_2_hex}/{full_hash}.json`
/// The model+version segment encodes the invalidation key, so switching backends
/// or updating weights causes an automatic miss without any bookkeeping.
fn result_cache_path(
    cache_root: &Path,
    model_name: &str,
    model_version: &str,
    text: &str,
) -> PathBuf {
    use xxhash_rust::xxh3::xxh3_64;
    let hash = format!("{:016x}", xxh3_64(text.as_bytes()));
    let shard = &hash[..2];
    let segment = format!(
        "{}-{}",
        model_name.replace(['/', '\\', ':'], "_"),
        model_version.replace(['/', '\\', ':'], "_"),
    );
    cache_root
        .join("results")
        .join(segment)
        .join(shard)
        .join(format!("{}.json", hash))
}

fn try_load_cached(path: &Path) -> Option<anno_core::GroundedDocument> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn store_cached(path: &Path, doc: &anno_core::GroundedDocument) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(doc) {
        let _ = std::fs::write(path, json);
    }
}


// Per-document extraction


struct DocOpts<'a> {
    coref: bool,
    link_kb: bool,
    cache_path: Option<PathBuf>,
    model: &'a dyn anno::Model,
}

fn process_document(
    doc_id: &str,
    text: &str,
    opts: &DocOpts<'_>,
) -> Result<anno_core::GroundedDocument, String> {
    use super::super::utils::{link_tracks_to_kb, resolve_coreference};
    use anno_core::{GroundedDocument, Location, Signal, SignalId};

    // Cache hit: return early without running extraction
    if let Some(ref path) = opts.cache_path {
        if let Some(doc) = try_load_cached(path) {
            return Ok(doc);
        }
    }

    let entities = opts
        .model
        .extract_entities(text, None)
        .map_err(|e| format!("Extraction failed for '{}': {}", doc_id, e))?;

    let mut doc = GroundedDocument::new(doc_id, text);
    let mut signal_ids: Vec<SignalId> = Vec::with_capacity(entities.len());

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

    if opts.coref {
        resolve_coreference(&mut doc, text, &signal_ids);
    }
    if opts.link_kb {
        link_tracks_to_kb(&mut doc);
    }

    // Cache miss: persist
    if let Some(ref path) = opts.cache_path {
        store_cached(path, &doc);
    }

    Ok(doc)
}


// Main entry point


/// Execute the batch processing command.
pub fn run(args: BatchArgs) -> Result<(), String> {
    use std::io::{self, BufRead};

    if args.dir.is_none() && !args.stdin {
        return Err("Either --dir <DIR> or --stdin must be specified".to_string());
    }
    if args.dir.is_some() && args.stdin {
        return Err("Cannot use both --dir and --stdin. Choose one.".to_string());
    }

    // Resolve cache root once (before model creation to avoid borrowing model_name later).
    let cache_root: Option<PathBuf> = if args.cache {
        Some(super::super::utils::get_cache_dir()?)
    } else {
        None
    };

    // Build the model, then wrap in Arc for cross-thread sharing.
    let model: Arc<Box<dyn anno::Model>> = Arc::new(args.model.create_model()?);
    let model_name = model.name().to_string();
    let model_version = model.version();

    // Collect (doc_id, text) pairs from the chosen input source.
    let inputs: Vec<(String, String)> = if args.stdin {
        if !args.quiet {
            eprintln!("Reading JSONL from stdin...");
        }
        let stdin = io::stdin();
        let mut out = Vec::new();
        for (i, line) in stdin.lock().lines().enumerate() {
            let line =
                line.map_err(|e| format!("Failed to read stdin line {}: {}", i + 1, e))?;
            if line.trim().is_empty() {
                continue;
            }
            let json: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| format!("Failed to parse stdin line {} as JSON: {}", i + 1, e))?;
            let doc_id = json
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("stdin:{}", i + 1));
            let text = json
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("Missing 'text' field in stdin line {}", i + 1))?
                .to_string();
            out.push((doc_id, text));
        }
        out
    } else {
        let dir = args.dir.as_ref().expect("validated above");
        let dir_path = Path::new(dir);
        let entries = std::fs::read_dir(dir_path)
            .map_err(|e| format!("Failed to read directory '{}': {}", dir, e))?;

        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext_ok = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "txt" || e == "md")
                .unwrap_or(false);
            if !ext_ok {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
            let doc_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("doc{}", out.len() + 1));
            out.push((doc_id, text));
        }

        if out.is_empty() {
            return Err(format!(
                "No input files found under '{}' (expected .txt or .md)",
                args.dir.as_deref().unwrap_or("")
            ));
        }
        out
    };

    if !args.quiet {
        let workers = if args.parallel > 1 {
            format!("{} workers", args.parallel)
        } else {
            "sequential".to_string()
        };
        let cache_note = if args.cache { ", cache on" } else { "" };
        eprintln!(
            "[batch] {} documents, model={}, {}{}",
            inputs.len(),
            model_name,
            workers,
            cache_note,
        );
    }

    // Progress bar setup (indicatif ProgressBar is Arc-backed, safe to clone for rayon).
    let pb = if args.progress && !args.quiet {
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(inputs.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .expect("valid template")
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };

    // Build per-document cache paths once (deterministic, parallel-safe).
    let cache_paths: Vec<Option<PathBuf>> = inputs
        .iter()
        .map(|(_, text)| {
            cache_root
                .as_ref()
                .map(|root| result_cache_path(root, &model_name, &model_version, text))
        })
        .collect();

    // Process documents — parallel when --parallel > 1, sequential otherwise.
    let documents: Vec<anno_core::GroundedDocument> = if args.parallel > 1 {
        use rayon::prelude::*;

        // Cap the rayon pool to the requested worker count.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(args.parallel)
            .build()
            .map_err(|e| format!("Failed to build thread pool: {}", e))?;

        let model_ref: Arc<Box<dyn anno::Model>> = Arc::clone(&model);
        let pb_ref = pb.clone();
        let results: Vec<Result<anno_core::GroundedDocument, String>> = pool.install(|| {
            inputs
                .par_iter()
                .zip(cache_paths.par_iter())
                .map(|((doc_id, text), cache_path)| {
                    let opts = DocOpts {
                        coref: args.coref,
                        link_kb: args.link_kb,
                        cache_path: cache_path.clone(),
                        model: model_ref.as_ref().as_ref(),
                    };
                    let result = process_document(doc_id, text, &opts);
                    if let Some(ref pb) = pb_ref {
                        pb.inc(1);
                    }
                    result
                })
                .collect()
        });

        // Collect, propagating the first error.
        results.into_iter().collect::<Result<Vec<_>, _>>()?
    } else {
        let mut docs = Vec::with_capacity(inputs.len());
        for ((doc_id, text), cache_path) in inputs.iter().zip(cache_paths.iter()) {
            if let Some(ref pb) = pb {
                pb.set_message(doc_id.clone());
            }
            let opts = DocOpts {
                coref: args.coref,
                link_kb: args.link_kb,
                cache_path: cache_path.clone(),
                model: model.as_ref().as_ref(),
            };
            docs.push(process_document(doc_id, text, &opts)?);
            if let Some(ref pb) = pb {
                pb.inc(1);
            }
        }
        docs
    };

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    // Write outputs.
    write_outputs(&documents, &args)?;

    if !args.quiet {
        let cached = cache_paths
            .iter()
            .filter(|p| p.as_ref().is_some_and(|p| p.exists()))
            .count();
        if args.cache && cached > 0 {
            eprintln!("[batch] {} cache hits, {} computed", cached, documents.len() - cached);
        }
        if let Some(ref out) = args.output {
            eprintln!("[batch] wrote {} document(s) to {}", documents.len(), out);
        }
    }

    Ok(())
}


// Output writing


fn write_outputs(
    documents: &[anno_core::GroundedDocument],
    args: &BatchArgs,
) -> Result<(), String> {
    use super::super::output::{color, print_signals};

    let Some(ref out_dir_str) = args.output else {
        // No output directory: print to stdout
        for doc in documents {
            if !args.quiet {
                println!("\n{}", color("1;36", &format!("Document: {}", doc.id)));
            }
            match args.format {
                OutputFormat::Json | OutputFormat::Grounded => {
                    let output = serde_json::to_string_pretty(doc)
                        .map_err(|e| format!("Failed to serialize '{}': {}", doc.id, e))?;
                    println!("{}", output);
                }
                _ => {
                    print_signals(doc, &doc.text, 0);
                }
            }
        }
        return Ok(());
    };

    let out_dir = PathBuf::from(out_dir_str);
    if out_dir.exists() && !out_dir.is_dir() {
        return Err(format!(
            "--output must be a directory for `anno batch`, but '{}' is not",
            out_dir.display()
        ));
    }
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Failed to create output dir '{}': {}", out_dir.display(), e))?;

    for doc in documents {
        match args.format {
            OutputFormat::Jsonl => {
                let path = out_dir.join(format!("{}.jsonl", doc.id));
                let payload = serde_json::to_string(doc)
                    .map_err(|e| format!("Failed to serialize '{}': {}", doc.id, e))?;
                std::fs::write(&path, payload + "\n")
                    .map_err(|e| format!("Failed to write '{}': {}", path.display(), e))?;
            }
            _ => {
                let path = out_dir.join(format!("{}.json", doc.id));
                let payload = serde_json::to_string_pretty(doc)
                    .map_err(|e| format!("Failed to serialize '{}': {}", doc.id, e))?;
                std::fs::write(&path, payload)
                    .map_err(|e| format!("Failed to write '{}': {}", path.display(), e))?;
            }
        }
    }

    Ok(())
}
