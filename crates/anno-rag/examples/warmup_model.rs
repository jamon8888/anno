//! Pre-download the embedder model used by `anno-rag`.
//!
//! Run once before integration tests or the first `ingest` call so the
//! ~470 MB `intfloat/multilingual-e5-small` weights land in the
//! HuggingFace cache (`~/.cache/huggingface/hub/`). After this, the
//! e2e test and the CLI `ingest` subcommand find the model on disk
//! and skip the download.
//!
//! ```sh
//! cargo run --example warmup_model -p anno-rag --release
//! ```

use anno_rag::config::AnnoRagConfig;
use hf_hub::api::tokio::Api;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnnoRagConfig::default();
    let api = Api::new()?;
    let repo = api.model(cfg.embed_model.clone());

    println!("Warming HF cache for {} ...", cfg.embed_model);

    for file in ["config.json", "tokenizer.json"] {
        print!("  {file} ... ");
        let path = repo.get(file).await?;
        let size = std::fs::metadata(&path)?.len();
        println!("ok ({:.1} KiB) -> {}", size as f64 / 1024.0, path.display());
    }

    // Weights live under one of two names; try the modern one first.
    print!("  weights ... ");
    let weights_path = match repo.get("model.safetensors").await {
        Ok(p) => p,
        Err(_) => repo
            .get("pytorch_model.bin")
            .await
            .map_err(|e| anyhow::anyhow!("neither model.safetensors nor pytorch_model.bin: {e}"))?,
    };
    let size_mb = std::fs::metadata(&weights_path)?.len() as f64 / 1024.0 / 1024.0;
    println!("ok ({size_mb:.1} MiB) -> {}", weights_path.display());

    println!();
    println!("Done. The model is now cached. Run integration tests with:");
    println!("  cargo test -p anno-rag --test e2e -- --ignored");
    Ok(())
}
