//! `anno-rag` CLI entrypoint.
//!
//! Subcommands (v0.2):
//! - `anno-rag ingest <folder> [--recursive] [--output <dir>]`
//! - `anno-rag search <query> [--top-k <N>]`
//! - `anno-rag mcp` — run MCP server on stdio (used by Cowork plugin)
//! - `anno-rag diagnose-gpu` — print accelerator diagnostics
//! - `anno-rag review <subcmd>` — tabular review management

mod review;

use anno_rag::{
    config::{AdvancedPdfNativeMode, AnnoRagConfig, MemoryNerMode, OcrMode},
    pipeline::Pipeline,
    vault::derive_key,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "anno-rag",
    version,
    about = "Local GDPR-compliant document anonymizer + RAG (French legal)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest documents from a folder. Writes pseudonymized copies to <output>
    /// and indexes embeddings in the local LanceDB store.
    Ingest {
        /// Folder to ingest.
        folder: PathBuf,
        /// Recurse into subfolders.
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        /// Where to write pseudonymized copies. Defaults to ~/.anno-rag/outputs.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Enable embedded OCR for scanned PDFs/pages when this binary was
        /// built with the `embedded-ocr` feature.
        #[arg(long, default_value_t = false)]
        enable_ocr: bool,
        /// Enable structured native PDF extraction for text-layer PDFs.
        #[arg(long, default_value_t = false)]
        advanced_pdf_native: bool,
        /// Keep running PDF headers in the advanced native PDF profile.
        #[arg(long, default_value_t = false)]
        pdf_keep_headers: bool,
        /// Keep running PDF footers in the advanced native PDF profile.
        #[arg(long, default_value_t = false)]
        pdf_keep_footers: bool,
        /// Extract PDF annotations in the advanced native PDF profile.
        #[arg(long, default_value_t = false)]
        pdf_extract_annotations: bool,
        /// Hierarchy cluster count for advanced native PDF extraction.
        #[arg(long, default_value_t = 6)]
        pdf_hierarchy_clusters: usize,
        /// Allow single-column pseudo-tables in advanced native PDF extraction.
        #[arg(long, default_value_t = false)]
        pdf_allow_single_column_tables: bool,
    },
    /// Search the indexed corpus and return ranked pseudonymized chunks.
    Search {
        /// Query text. May contain PII — will be pseudonymized through the vault.
        query: String,
        /// Number of results.
        #[arg(short = 'k', long, default_value_t = 10)]
        top_k: usize,
    },
    /// Run the MCP server on stdio. Used by Cowork as a plugin transport.
    /// Blocks until stdin closes.
    Mcp,
    /// Print GPU/accelerator diagnostics without loading model weights.
    DiagnoseGpu,
    /// Reproduce SLO measurements on a user corpus.
    Bench {
        /// Folder containing the documents to bench (PDF/DOCX/TXT/MD).
        #[arg(long, value_name = "DIR")]
        corpus: PathBuf,
    },
    /// Download model weights to a local directory and print the path.
    /// Set ANNO_MODELS_DIR to the printed path so anno-rag works offline.
    DownloadModels {
        /// Directory to download into. Defaults to ~/.anno-rag/models.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Vault admin: keyring status, rotation (spec §14.4).
    Vault {
        #[command(subcommand)]
        sub: VaultCmd,
    },
    /// Tabular review management: create, list, add rows, run extraction, export.
    Review(review::ReviewArgs),
}

#[derive(Subcommand)]
enum VaultCmd {
    /// Report whether the OS keyring holds a vault key entry.
    /// Never echoes the key itself.
    Status,
    /// Generate a new random vault key and replace the keyring entry.
    /// Requires an existing entry; fails otherwise.
    Rotate,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Bench builds its own Pipeline in a tempdir; short-circuit before keyring lookup.
    if let Cmd::Bench { corpus } = &cli.cmd {
        anno_rag::bench_cli::run(corpus).await?;
        return Ok(());
    }

    // Initialize cfg early so it's available for short-circuit branches
    let mut cfg = AnnoRagConfig::default();
    if let Ok(mode) = std::env::var("ANNO_RAG_MEMORY_NER_MODE") {
        if let Some(mode) = MemoryNerMode::from_env_value(&mode) {
            cfg.memory_ner_mode = mode;
        }
    }
    if let Cmd::Ingest {
        enable_ocr,
        advanced_pdf_native,
        pdf_keep_headers,
        pdf_keep_footers,
        pdf_extract_annotations,
        pdf_hierarchy_clusters,
        pdf_allow_single_column_tables,
        ..
    } = &cli.cmd
    {
        if *enable_ocr {
            cfg.ocr_mode = OcrMode::AutoEmbedded;
        }
        if *advanced_pdf_native {
            cfg.advanced_pdf_native = AdvancedPdfNativeMode::Structured;
        }
        cfg.pdf_keep_headers = *pdf_keep_headers;
        cfg.pdf_keep_footers = *pdf_keep_footers;
        cfg.pdf_extract_annotations = *pdf_extract_annotations;
        cfg.pdf_hierarchy_clusters = *pdf_hierarchy_clusters;
        cfg.pdf_allow_single_column_tables = *pdf_allow_single_column_tables;
    }

    if let Cmd::DiagnoseGpu = &cli.cmd {
        let diagnostics = anno_rag::accelerator::diagnostics(cfg.accelerator)?;
        println!("{}", serde_json::to_string_pretty(&diagnostics)?);
        return Ok(());
    }

    // Mcp uses lazy pipeline init — short-circuit before Pipeline::new.
    // Auto-detection of the default models path is handled inside pipeline()
    // in anno-rag-mcp to avoid env::set_var in a multi-threaded context.
    if let Cmd::Mcp = &cli.cmd {
        let key = derive_key()?;
        anno_rag_mcp::serve_stdio_lazy(cfg, key).await?;
        return Ok(());
    }

    // Vault admin short-circuits before Pipeline::new to avoid Path A
    // auto-generation during `vault status`.
    if let Cmd::Vault { sub } = &cli.cmd {
        match sub {
            VaultCmd::Status => {
                let s = anno_rag::vault_admin::vault_status()?;
                println!("{}", serde_json::to_string_pretty(&s)?);
            }
            VaultCmd::Rotate => {
                anno_rag::vault_admin::vault_rotate()?;
                println!("{{\"ok\": true}}");
            }
        }
        return Ok(());
    }

    // Review subcommands short-circuit before Pipeline::new.
    // The `extract` subcommand builds its own pipeline internally when needed.
    // We check by reference first (borrows are released after `matches!` returns)
    // and then take ownership with `let-else`; after the block only the false
    // path remains live, so `cli.cmd` is still available for the final `match`.
    if matches!(&cli.cmd, Cmd::Review(_)) {
        let Cmd::Review(args) = cli.cmd else {
            unreachable!()
        };
        review::run(args.cmd, &cfg).await?;
        return Ok(());
    }

    // DownloadModels needs no Pipeline — short-circuit before keyring lookup.
    if let Cmd::DownloadModels { dir } = &cli.cmd {
        let mut cfg = AnnoRagConfig::default();
        if let Some(d) = dir {
            // models_cache() = data_dir/models, so set data_dir = d.parent()
            // which makes models_cache() == d (when d ends in "models").
            cfg.data_dir = d
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| d.clone());
        }
        println!(
            "Downloading anno-rag models to: {}",
            cfg.models_cache().display()
        );
        println!("  (embedder ~470 MiB + NER ~500 MiB = ~970 MiB total)");
        println!();
        let models_dir = anno_rag::download_models::download(&cfg).await?;
        println!();
        println!("Done. Set the following environment variable:");
        println!();
        #[cfg(windows)]
        println!("  $env:ANNO_MODELS_DIR = \"{}\"", models_dir.display());
        #[cfg(not(windows))]
        println!("  export ANNO_MODELS_DIR=\"{}\"", models_dir.display());
        println!();
        println!("Or add it permanently to your shell profile / Claude Desktop config env.");
        return Ok(());
    }
    let key = derive_key()?;
    let pipeline = Pipeline::new(cfg.clone(), key).await?;

    match cli.cmd {
        Cmd::Ingest {
            folder,
            recursive,
            output,
            enable_ocr: _,
            advanced_pdf_native: _,
            pdf_keep_headers: _,
            pdf_keep_footers: _,
            pdf_extract_annotations: _,
            pdf_hierarchy_clusters: _,
            pdf_allow_single_column_tables: _,
        } => {
            let out = output.unwrap_or_else(|| cfg.outputs_dir());
            let n = pipeline.ingest_folder(&folder, recursive, &out).await?;
            println!("ingested {n} documents → {}", out.display());
        }
        Cmd::Search { query, top_k } => {
            let hits = pipeline.search(&query, top_k).await?;
            if hits.is_empty() {
                println!("(no results)");
            }
            for (i, h) in hits.iter().enumerate() {
                println!("#{} score={:.3} page={:?}", i + 1, h.score, h.page);
                println!("    source: {}", h.source_path);
                println!("    text:   {}", truncate(&h.text_pseudo, 200));
                println!();
            }
        }
        Cmd::Mcp => unreachable!("handled above before Pipeline::new"),
        Cmd::DiagnoseGpu => unreachable!("handled above before Pipeline::new"),
        Cmd::Bench { .. } => unreachable!("handled above before Pipeline::new"),
        Cmd::DownloadModels { .. } => unreachable!("handled above before Pipeline::new"),
        Cmd::Vault { .. } => unreachable!("handled above before Pipeline::new"),
        Cmd::Review(_) => unreachable!("handled above before Pipeline::new"),
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

#[cfg(test)]
mod mcp_autodetect_tests {
    use anno_rag::config::AnnoRagConfig;

    #[test]
    fn models_cache_path_matches_default_autodetect_layout() {
        let cfg = AnnoRagConfig::default();
        let models = cfg.models_cache();
        assert!(models.ends_with("models"));
        assert_eq!(
            models.join("multilingual-e5-small").file_name().unwrap(),
            "multilingual-e5-small"
        );
        assert_eq!(
            models.join("gliner2-multi-v1-onnx").file_name().unwrap(),
            "gliner2-multi-v1-onnx"
        );
    }
}

#[cfg(test)]
mod gpu_cli_tests {
    use super::*;

    #[test]
    fn parses_diagnose_gpu_command() {
        let cli = Cli::try_parse_from(["anno-rag", "diagnose-gpu"]).expect("parse");
        assert!(matches!(cli.cmd, Cmd::DiagnoseGpu));
    }
}
