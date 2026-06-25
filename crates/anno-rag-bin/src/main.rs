//! `anno-rag` CLI entrypoint.
//!
//! Subcommands (v0.2):
//! - `anno-rag ingest <folder> [--recursive] [--output <dir>]`
//! - `anno-rag search <query> [--top-k <N>]`
//! - `anno-rag mcp` — run MCP server on stdio (used by Cowork plugin)
//! - `anno-rag diagnose-gpu` — print accelerator diagnostics
//! - `anno-rag review <subcmd>` — tabular review management

mod config_cmd;
mod review;
mod setup_mcp;

use anno_rag::{
    config::{AnnoRagConfig, OcrMode},
    pipeline::Pipeline,
    vault::derive_key,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing;

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
        /// Config overrides — any AnnoRagConfig field settable here (e.g. --ocr-mode, --gdpr-layers).
        #[command(flatten)]
        config: anno_rag::config::ConfigOverrides,
        /// Index profile for the registered corpus: all (default), legal, or general.
        #[arg(long, default_value = "all")]
        profile: String,
        /// Optional human-readable corpus alias (e.g. a matter number like 2026-0042).
        #[arg(long)]
        alias: Option<String>,
    },
    /// Search the indexed corpus and return ranked pseudonymized chunks.
    Search {
        /// Query text. May contain PII — will be pseudonymized through the vault.
        query: String,
        /// Number of results.
        #[arg(short = 'k', long, default_value_t = 10)]
        top_k: usize,
        /// Config overrides.
        #[command(flatten)]
        config: anno_rag::config::ConfigOverrides,
    },
    /// Config management: init, show, validate.
    Config {
        #[command(subcommand)]
        sub: ConfigSubCmd,
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
    /// Configure local MCP clients for Claude Desktop/Cowork and Claude Code.
    SetupMcp(setup_mcp::SetupMcpArgs),
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

#[derive(Subcommand)]
enum ConfigSubCmd {
    /// Create ~/.anno-rag/config.toml from the built-in template.
    Init {
        /// Custom path for the config file (default: ~/.anno-rag/config.toml).
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Print effective configuration with source annotation.
    Show {
        /// Custom config file path.
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Validate config.toml without starting the pipeline.
    Validate {
        /// Config file path (default: ~/.anno-rag/config.toml).
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Bench builds its own Pipeline in a tempdir; short-circuit before keyring lookup.
    if let Cmd::Bench { corpus } = &cli.cmd {
        anno_rag::bench_cli::run(corpus).await?;
        return Ok(());
    }

    // Extract CLI overrides from subcommand, then load: defaults → TOML → env → overrides.
    let config_overrides = match &cli.cmd {
        Cmd::Ingest { config, .. } => Some(config.clone()),
        Cmd::Search { config, .. } => Some(config.clone()),
        _ => None,
    };
    let mut cfg = AnnoRagConfig::load(config_overrides.as_ref()).unwrap_or_else(|e| {
        tracing::warn!("config load error: {e}; using defaults");
        AnnoRagConfig::default()
    });

    // --enable-ocr / ANNO_RAG_ENABLE_OCR: deprecated flag, promote to ocr_mode.
    if cfg.enable_ocr && cfg.ocr_mode == OcrMode::Off {
        tracing::warn!("--enable-ocr / ANNO_RAG_ENABLE_OCR is deprecated; use --ocr-mode auto_embedded instead");
        cfg.ocr_mode = OcrMode::AutoEmbedded;
    }

    cfg.warn_deprecated_fields();

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

    if let Cmd::Config { sub } = &cli.cmd {
        match sub {
            ConfigSubCmd::Init { path } => {
                let p = path
                    .clone()
                    .or_else(AnnoRagConfig::default_config_path)
                    .ok_or_else(|| anyhow::anyhow!("cannot determine config path"))?;
                config_cmd::config_init(&p)?;
            }
            ConfigSubCmd::Show { path } => {
                config_cmd::config_show(path.as_deref())?;
            }
            ConfigSubCmd::Validate { path } => {
                let p = path
                    .clone()
                    .or_else(AnnoRagConfig::default_config_path)
                    .ok_or_else(|| anyhow::anyhow!("cannot determine config path"))?;
                config_cmd::config_validate(&p)?;
            }
        }
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

    if matches!(&cli.cmd, Cmd::SetupMcp(_)) {
        let Cmd::SetupMcp(args) = cli.cmd else {
            unreachable!()
        };
        setup_mcp::run(args).await?;
        return Ok(());
    }

    // DownloadModels needs no Pipeline — short-circuit before keyring lookup.
    if let Cmd::DownloadModels { dir } = &cli.cmd {
        let mut cfg = AnnoRagConfig::load(None).unwrap_or_else(|e| {
            tracing::warn!("config load error: {e}; using defaults");
            AnnoRagConfig::default()
        });
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
    let mut pipeline = Pipeline::new(cfg.clone(), key).await?;

    #[cfg(feature = "vlm-ocr")]
    {
        match anno_rag_tabular::llm::vlm::routing::RoutingVlmClient::from_config(&cfg) {
            Ok(Some(client)) => {
                pipeline.set_vlm_client(std::sync::Arc::new(client));
            }
            Ok(None) => {
                // vlm_backend = "off" — fall through to Tesseract
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "VLM client init failed, falling back to Tesseract"
                );
            }
        }
    }

    match cli.cmd {
        Cmd::Ingest {
            folder,
            recursive,
            output,
            config: _,
            profile,
            alias,
        } => {
            let out = output.unwrap_or_else(|| cfg.outputs_dir());
            // The CLI ingest path is legal-scoped; reject other profiles.
            if profile != "legal" {
                return Err(anyhow::anyhow!(
                    "ingest profile must be 'legal', got '{profile}'. \
                     Use --profile legal for document-scoped ingestion."
                ));
            }
            // Register the folder as a corpus so search resolution + document
            // handles work for documents ingested via the CLI.
            let svc = anno_rag_mcp::corpus::CorpusService::open(&cfg)?;
            let reg = svc.register_index_root(&folder.to_string_lossy(), &profile)?;
            if let Some(alias) = alias.as_deref() {
                svc.store().set_alias(reg.corpus_id, alias)?;
            }
            let scope = anno_rag::LegalIngestScope {
                corpus_id: reg.corpus_id,
                root: folder.clone(),
            };
            let summary = pipeline
                .ingest_folder_scoped_summary(&folder, recursive, &out, scope)
                .await?;
            let corpus_id = reg.corpus_id;
            for doc in &summary.documents {
                svc.store().add_document(
                    corpus_id,
                    doc.document_id,
                    "legal",
                    &doc.source_path,
                    doc.relative_path.as_deref(),
                    &doc.content_id,
                    &serde_json::json!({}),
                )?;
            }
            println!(
                "ingested {} documents → {} (corpus {})",
                summary.ingested,
                out.display(),
                reg.corpus_id.as_string()
            );
        }
        Cmd::Search {
            query,
            top_k,
            config: _,
        } => {
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
        Cmd::SetupMcp(_) => unreachable!("handled above before Pipeline::new"),
        Cmd::Vault { .. } => unreachable!("handled above before Pipeline::new"),
        Cmd::Review(_) => unreachable!("handled above before Pipeline::new"),
        Cmd::Config { .. } => unreachable!("handled above before Pipeline::new"),
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
        let embed_dir = cfg.embedder_dir();
        assert_eq!(
            models.join(&embed_dir).file_name().unwrap(),
            embed_dir.as_str()
        );
        let ner_dir = cfg.ner_onnx_dir();
        assert_eq!(models.join(&ner_dir).file_name().unwrap(), ner_dir.as_str());
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

#[cfg(test)]
mod setup_mcp_cli_tests {
    use super::*;

    #[test]
    fn parses_setup_mcp_command() {
        let cli = Cli::try_parse_from([
            "anno-rag",
            "setup-mcp",
            "--target",
            "manual",
            "--binary",
            "C:/Tools/hacienda/anno-rag.exe",
        ])
        .expect("parse");

        assert!(matches!(cli.cmd, Cmd::SetupMcp(_)));
    }

    #[test]
    fn parses_setup_mcp_scope_and_desktop_mode() {
        let cli = Cli::try_parse_from([
            "anno-rag",
            "setup-mcp",
            "--target",
            "claude-code",
            "--desktop-mode",
            "mcpb",
            "--claude-code-scope",
            "project",
            "--dry-run",
        ])
        .expect("parse");

        let Cmd::SetupMcp(args) = cli.cmd else {
            panic!("expected setup-mcp");
        };
        assert_eq!(args.target, setup_mcp::SetupTarget::ClaudeCode);
        assert_eq!(args.desktop_mode, setup_mcp::DesktopMode::Mcpb);
        assert_eq!(args.claude_code_scope, setup_mcp::ClaudeCodeScope::Project);
        assert!(args.dry_run);
    }
}
