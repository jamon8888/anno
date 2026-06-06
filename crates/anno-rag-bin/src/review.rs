//! `anno-rag review` subcommand — tabular review CLI.
//!
//! Provides five subcommands:
//!
//! | Subcommand  | Purpose                                        |
//! |-------------|------------------------------------------------|
//! | `list`      | List all reviews in the local store            |
//! | `create`    | Create a new review (optionally from template) |
//! | `add-rows`  | Register document rows for extraction          |
//! | `extract`   | Run LLM extraction over all rows               |
//! | `export`    | Dump the filled grid as CSV / XLSX / Markdown  |
//!
//! The first four subcommands short-circuit before `Pipeline::new` — they
//! only open the LanceDB tabular tables, avoiding the model-weight loads.
//! `extract` is the only one that boots the full pipeline (needed for the
//! `ChunkSource` adapter).

use anno_rag::config::AnnoRagConfig;
use anno_rag_tabular::{
    error::Error as TabularError,
    export::{export_csv, export_markdown, export_xlsx},
    extract::{ChunkRef, ChunkSource, Extractor},
    fanout::{run_review, FanoutConfig},
    ids::{ReviewId, RowId},
    schema::template::Template,
    storage::{reviews::Review, rows::Row, StorageHandle},
};
use async_trait::async_trait;
use clap::Subcommand;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Open the tabular LanceDB storage without touching the anno-rag pipeline.
///
/// Uses the same `index.lance` database that the pipeline uses, so both
/// the RAG corpus tables and the tabular-review tables live side by side
/// in the same Lance dataset directory.
async fn open_tabular_storage(cfg: &AnnoRagConfig) -> anyhow::Result<StorageHandle> {
    let path = cfg.index_path();
    let uri = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("data_dir contains non-UTF-8 characters"))?;
    let conn = Arc::new(lancedb::connect(uri).execute().await?);
    Ok(StorageHandle::open(conn).await?)
}

// ── Clap argument types ───────────────────────────────────────────────────────

/// Arguments for `anno-rag review …`.
#[derive(clap::Args)]
pub struct ReviewArgs {
    #[command(subcommand)]
    pub cmd: ReviewCmd,
}

/// Available `review` subcommands.
#[derive(Subcommand)]
pub enum ReviewCmd {
    /// List all reviews stored locally.
    List,

    /// Create a new review, optionally seeding columns from a built-in template.
    Create {
        /// Human-readable display name, e.g. `"Acme NDA batch"`.
        #[arg(long)]
        name: String,
        /// Built-in template id (e.g. `nda-v1`, `customer-contract-v1`).
        /// Omit to create an empty review and add columns manually.
        #[arg(long)]
        template: Option<String>,
    },

    /// Register one or more document rows for a review.
    ///
    /// Each `--doc-id` is an anno-rag document UUID returned by `ingest`
    /// or visible in `search` output. The `--folder-path` value is stored
    /// as metadata on every row and used as the row label in exports.
    AddRows {
        /// Target review UUID.
        #[arg(long)]
        review: Uuid,
        /// Folder path label stored on each row (e.g. `"Deal_Acme/01_NDA"`).
        #[arg(long)]
        folder_path: String,
        /// Comma-separated document UUIDs to add.
        #[arg(long, value_delimiter = ',')]
        doc_ids: Vec<Uuid>,
    },

    /// Run LLM extraction for all rows in a review.
    ///
    /// Requires `ANTHROPIC_API_KEY` (or another LLM env var) to be set.
    /// Boots the full anno-rag pipeline to source pseudonymized chunks, so
    /// model weights must be available.
    Extract {
        /// Target review UUID.
        #[arg(long)]
        review: Uuid,
        /// Re-extract columns that already have a cell (skip incremental
        /// optimisation). Useful after editing a column prompt.
        #[arg(long, default_value_t = false)]
        force: bool,
        /// Allow a remote LLM fallback for columns the local model cannot
        /// fill. OFF by default: extraction stays 100% local unless set.
        /// The remote prompt is still gated by the PII safety check.
        #[arg(long, default_value_t = false)]
        allow_remote_llm: bool,
    },

    /// Export a filled review to CSV, XLSX, or Markdown.
    Export {
        /// Target review UUID.
        #[arg(long)]
        review: Uuid,
        /// Output format: `csv` (default), `xlsx`, or `md`.
        #[arg(long, default_value = "csv")]
        format: String,
        /// Output file path. Defaults to stdout for `csv` and `md`.
        /// Required for `xlsx`.
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

/// Entry point called from `main.rs` after clap parsing.
///
/// `cfg` is already initialised (env vars applied) but `Pipeline::new` has
/// NOT been called yet — that is the caller's responsibility when needed
/// (only the `extract` subcommand requires it, and this function handles
/// that itself).
pub async fn run(cmd: ReviewCmd, cfg: &AnnoRagConfig) -> anyhow::Result<()> {
    match cmd {
        ReviewCmd::List => cmd_list(cfg).await,
        ReviewCmd::Create { name, template } => cmd_create(cfg, name, template).await,
        ReviewCmd::AddRows {
            review,
            folder_path,
            doc_ids,
        } => cmd_add_rows(cfg, ReviewId(review), folder_path, doc_ids).await,
        ReviewCmd::Extract {
            review,
            force,
            allow_remote_llm,
        } => cmd_extract(cfg, ReviewId(review), force, allow_remote_llm).await,
        ReviewCmd::Export {
            review,
            format,
            output,
        } => cmd_export(cfg, ReviewId(review), format, output).await,
    }
}

// ── Subcommand implementations ────────────────────────────────────────────────

async fn cmd_list(cfg: &AnnoRagConfig) -> anyhow::Result<()> {
    let storage = open_tabular_storage(cfg).await?;
    let reviews = storage.reviews.list().await?;
    if reviews.is_empty() {
        println!("(no reviews)");
        return Ok(());
    }
    for r in &reviews {
        println!(
            "{} — {} (schema_version={})",
            r.id.0, r.name, r.schema_version
        );
        if let Some(t) = &r.template_id {
            println!("   template: {t}");
        }
        if let Some(f) = &r.scope_folder {
            println!("   scope_folder: {f}");
        }
        let row_count = storage.rows.list_for_review(r.id).await?.len();
        println!("   rows: {row_count}");
    }
    println!();
    println!("{} review(s)", reviews.len());
    Ok(())
}

async fn cmd_create(
    cfg: &AnnoRagConfig,
    name: String,
    template: Option<String>,
) -> anyhow::Result<()> {
    let storage = open_tabular_storage(cfg).await?;
    let review_id = ReviewId::new();
    let review = Review {
        id: review_id,
        name: name.clone(),
        project_id: None,
        template_id: template.clone(),
        scope_folder: None,
        created_at: chrono::Utc::now(),
        schema_version: 1,
    };
    storage.reviews.create(&review).await?;
    if let Some(tmpl_id) = template {
        let tmpl = Template::builtin(&tmpl_id)
            .map_err(|e| anyhow::anyhow!("unknown template '{tmpl_id}': {e}"))?;
        let cols = tmpl.into_columns(review_id);
        let n = cols.len();
        for col in &cols {
            storage.columns.add(review_id, col).await?;
        }
        println!(
            "Created review {} ({name}) — {n} columns from template '{tmpl_id}'",
            review_id.0
        );
    } else {
        println!("Created review {} ({name})", review_id.0);
        println!("Hint: add document rows with `anno-rag review add-rows`. Columns are fixed at creation time — to change the schema, create a new review with a different template or --schema.");
    }
    Ok(())
}

async fn cmd_add_rows(
    cfg: &AnnoRagConfig,
    review_id: ReviewId,
    folder_path: String,
    doc_ids: Vec<Uuid>,
) -> anyhow::Result<()> {
    if doc_ids.is_empty() {
        anyhow::bail!("--doc-ids must contain at least one UUID");
    }
    let storage = open_tabular_storage(cfg).await?;
    // Verify the review exists.
    storage
        .reviews
        .get(review_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("review {} not found", review_id.0))?;

    let mut added = 0usize;
    for doc_id in &doc_ids {
        let row = Row {
            id: RowId::for_doc(review_id, *doc_id),
            review_id,
            doc_id: *doc_id,
            folder_path: Some(folder_path.clone()),
            created_at: chrono::Utc::now(),
        };
        storage.rows.add(&row).await?;
        added += 1;
    }
    println!(
        "Added {added} row(s) to review {} (folder_path='{folder_path}')",
        review_id.0
    );
    Ok(())
}

async fn cmd_extract(
    cfg: &AnnoRagConfig,
    review_id: ReviewId,
    force: bool,
    allow_remote_llm: bool,
) -> anyhow::Result<()> {
    // Extract requires the full pipeline (chunk source) + an LLM client.
    let key = anno_rag::vault::derive_key()?;
    let pipeline = anno_rag::pipeline::Pipeline::new(cfg.clone(), key).await?;
    let pipeline = Arc::new(pipeline);

    // Open a second LanceDB connection for the tabular tables.
    // NOTE: LanceDB's Rust SDK is append-only; concurrent *readers* on the
    // same directory are safe.  Concurrent *writers* (e.g. a background MCP
    // extraction running alongside a CLI `extract` call) may produce duplicate
    // version rows — `latest()` will return the most-recent write, so no data
    // is lost, but callers should not run two extraction processes in parallel.
    let storage = open_tabular_storage(cfg).await?;

    let columns = storage.columns.list_for_review(review_id).await?;
    let rows = storage.rows.list_for_review(review_id).await?;
    if columns.is_empty() {
        anyhow::bail!("review {} has no columns — add columns first", review_id.0);
    }
    if rows.is_empty() {
        anyhow::bail!(
            "review {} has no rows — add document rows with `add-rows` first",
            review_id.0
        );
    }
    println!(
        "Extracting {} column(s) × {} row(s) for review {}…",
        columns.len(),
        rows.len(),
        review_id.0
    );

    let chunks = Arc::new(CliChunkSource(Arc::clone(&pipeline)));
    if allow_remote_llm {
        eprintln!("⚠  Remote LLM fallback ENABLED — pseudonymized prompts may transit to the remote API (PII-gated).");
    } else {
        eprintln!("🔒 Local-only extraction (no remote LLM). Pass --allow-remote-llm to enable a PII-gated fallback.");
    }
    let llm_box = anno_rag_tabular::llm::routing_client_from_env(allow_remote_llm)
        .map_err(|e| anyhow::anyhow!("LLM client init failed: {e}"))?;
    let llm: Arc<dyn anno_rag_tabular::llm::LlmClient> = Arc::from(llm_box);
    let extractor = Extractor::new(llm, chunks);
    let fanout_cfg = FanoutConfig {
        force_reextract: force,
        ..Default::default()
    };

    let outcomes = run_review(&storage, &extractor, review_id, fanout_cfg).await?;
    let ok: usize = outcomes.iter().filter(|o| o.result.is_ok()).count();
    let err: usize = outcomes.len() - ok;
    println!("Done — {ok} row(s) ok, {err} row(s) failed.");
    for o in outcomes.iter().filter(|o| o.result.is_err()) {
        eprintln!(
            "  row {} doc {}: {}",
            o.row_id.0,
            o.doc_id,
            o.result.as_ref().unwrap_err()
        );
    }
    if err > 0 {
        anyhow::bail!("{err} row(s) failed during extraction");
    }
    Ok(())
}

async fn cmd_export(
    cfg: &AnnoRagConfig,
    review_id: ReviewId,
    format: String,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let storage = open_tabular_storage(cfg).await?;
    match format.to_lowercase().as_str() {
        "csv" => {
            let content = export_csv(&storage, review_id).await?;
            if let Some(path) = output {
                std::fs::write(&path, &content)?;
                println!("Exported CSV → {}", path.display());
            } else {
                print!("{content}");
            }
        }
        "xlsx" => {
            let path = output
                .ok_or_else(|| anyhow::anyhow!("--output <path> is required for xlsx format"))?;
            export_xlsx(&storage, review_id, &path).await?;
            println!("Exported XLSX → {}", path.display());
        }
        "md" | "markdown" => {
            let content = export_markdown(&storage, review_id).await?;
            if let Some(path) = output {
                std::fs::write(&path, &content)?;
                println!("Exported Markdown → {}", path.display());
            } else {
                print!("{content}");
            }
        }
        _ => {
            anyhow::bail!(
                "unknown export format '{}'; valid values: csv, md, xlsx",
                format
            );
        }
    }
    Ok(())
}

// ── CliChunkSource ────────────────────────────────────────────────────────────

/// Minimal [`ChunkSource`] adapter for the CLI `extract` subcommand.
///
/// Wraps [`anno_rag::pipeline::Pipeline`] to fetch pseudonymized chunks
/// from the existing anno-rag index. Mirrors [`anno_rag_mcp::tabular::PipelineChunkSource`]
/// but lives here to avoid a dep cycle through the `anno-rag-bin → anno-rag-mcp`
/// edge.
struct CliChunkSource(Arc<anno_rag::pipeline::Pipeline>);

#[async_trait]
impl ChunkSource for CliChunkSource {
    async fn chunks_for_doc(&self, doc_id: Uuid) -> anno_rag_tabular::error::Result<Vec<ChunkRef>> {
        let hits = self
            .0
            .chunks_by_doc(doc_id)
            .await
            .map_err(|e| TabularError::Extract {
                doc: doc_id.to_string(),
                col: "?".into(),
                source: Box::new(e),
            })?;
        Ok(hits
            .into_iter()
            .map(|h| ChunkRef {
                id: h.chunk_id,
                doc_id: h.doc_id,
                content: h.text_pseudo,
                page: h.page,
            })
            .collect())
    }

    async fn chunk_by_id(
        &self,
        chunk_id: Uuid,
    ) -> anno_rag_tabular::error::Result<Option<ChunkRef>> {
        let hit = self
            .0
            .chunk_by_id(chunk_id)
            .await
            .map_err(|e| TabularError::Extract {
                doc: "?".into(),
                col: "?".into(),
                source: Box::new(e),
            })?;
        Ok(hit.map(|h| ChunkRef {
            id: h.chunk_id,
            doc_id: h.doc_id,
            content: h.text_pseudo,
            page: h.page,
        }))
    }
}
