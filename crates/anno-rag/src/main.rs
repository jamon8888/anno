//! `anno-rag` CLI entrypoint.
//!
//! v0.1 walking skeleton: two subcommands.
//! - `anno-rag ingest <folder> [--recursive] [--output <dir>]`
//! - `anno-rag search <query> [--top-k <N>]`

use anno_rag::{config::AnnoRagConfig, pipeline::Pipeline, vault::derive_key};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anno-rag", version, about = "Local GDPR-compliant document anonymizer + RAG (French legal)")]
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
    },
    /// Search the indexed corpus and return ranked pseudonymized chunks.
    Search {
        /// Query text. May contain PII — will be pseudonymized through the vault.
        query: String,
        /// Number of results.
        #[arg(short = 'k', long, default_value_t = 10)]
        top_k: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cfg = AnnoRagConfig::default();
    let key = derive_key()?;
    let pipeline = Pipeline::new(cfg.clone(), key).await?;

    match cli.cmd {
        Cmd::Ingest { folder, recursive, output } => {
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
                println!(
                    "#{} distance={:.3} page={:?}",
                    i + 1,
                    h.distance,
                    h.page
                );
                println!("    source: {}", h.source_path);
                println!("    text:   {}", truncate(&h.text_pseudo, 200));
                println!();
            }
        }
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
