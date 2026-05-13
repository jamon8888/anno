//! Pre-download both the embedder AND the anno NER model used by `anno-rag`.
//!
//! Run once before integration tests or the first CLI use:
//!
//! ```sh
//! cargo run --example warmup_model -p anno-rag --release
//! ```
//!
//! Total cache: ~950 MiB (intfloat/multilingual-e5-small embedder ~448 MiB
//! + GLiNER2Fastino multi-v1 ONNX ~500 MiB).

use anno_rag::config::AnnoRagConfig;
use hf_hub::api::tokio::Api;

/// Single NER backend as of v0.5 #025 (T4): GLiNER2Fastino multi-v1 ONNX.
/// Replaces the previous `StackedNER` multi-candidate fallback chain.
const NER_MODEL_ID: &str = "SemplificaAI/gliner2-multi-v1-onnx";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnnoRagConfig::default();
    let api = Api::new()?;

    // ---- 1. Embedder ----
    println!("Warming embedder: {}", cfg.embed_model);
    warm_embedder(&api, &cfg.embed_model).await?;

    // ---- 2. NER (single backend, GLiNER2Fastino multi-v1) ----
    println!();
    println!("Downloading NER model: {NER_MODEL_ID}");
    anno::backends::gliner2_fastino::GLiNER2Fastino::from_pretrained(NER_MODEL_ID)?;
    println!("NER model cached.");

    println!();
    println!("Done. Run integration tests with:");
    println!("  cargo test -p anno-rag --test e2e -- --ignored");
    Ok(())
}

async fn warm_embedder(api: &Api, model_id: &str) -> anyhow::Result<()> {
    let repo = api.model(model_id.to_string());

    for file in ["config.json", "tokenizer.json"] {
        let path = repo.get(file).await?;
        let size = std::fs::metadata(&path)?.len();
        println!(
            "  {file} ... ok ({:.1} KiB) -> {}",
            size as f64 / 1024.0,
            path.display()
        );
    }

    // Weights — try modern format first.
    let weights_path = match repo.get("model.safetensors").await {
        Ok(p) => p,
        Err(_) => repo
            .get("pytorch_model.bin")
            .await
            .map_err(|e| anyhow::anyhow!("neither model.safetensors nor pytorch_model.bin: {e}"))?,
    };
    let size_mb = std::fs::metadata(&weights_path)?.len() as f64 / 1024.0 / 1024.0;
    println!("  weights ... ok ({size_mb:.1} MiB) -> {}", weights_path.display());
    Ok(())
}
