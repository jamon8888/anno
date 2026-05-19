//! `anno-rag` CLI entrypoint.
//!
//! Subcommands (v0.2):
//! - `anno-rag ingest <folder> [--recursive] [--output <dir>]`
//! - `anno-rag search <query> [--top-k <N>]`
//! - `anno-rag mcp` — run MCP server on stdio (used by Cowork plugin)

use anno_rag::{
    config::{AnnoRagConfig, OcrMode},
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
    /// Reproduce SLO measurements on a user corpus.
    Bench {
        /// Folder containing the documents to bench (PDF/DOCX/TXT/MD).
        #[arg(long, value_name = "DIR")]
        corpus: PathBuf,
    },
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

    let mut cfg = AnnoRagConfig::default();
    if let Cmd::Ingest { enable_ocr, .. } = &cli.cmd {
        if *enable_ocr {
            cfg.ocr_mode = OcrMode::AutoEmbedded;
        }
    }
    let key = derive_key()?;
    let pipeline = Pipeline::new(cfg.clone(), key).await?;

    match cli.cmd {
        Cmd::Ingest {
            folder,
            recursive,
            output,
            enable_ocr: _,
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
        Cmd::Mcp => {
            anno_rag_mcp::serve_stdio(pipeline, cfg).await?;
        }
        Cmd::Bench { .. } => unreachable!("handled above before Pipeline::new"),
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
