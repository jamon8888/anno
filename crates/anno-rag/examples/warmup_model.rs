//! Pre-download both the embedder AND an anno NER model used by `anno-rag`.
//!
//! Run once before integration tests or the first CLI use:
//!
//! ```sh
//! cargo run --example warmup_model -p anno-rag --release
//! ```
//!
//! Total cache: ~600 MiB (intfloat/multilingual-e5-small embedder
//! ~448 MiB + an anno NER model ~70-150 MiB depending on which
//! candidate succeeds first).

use anno_rag::config::AnnoRagConfig;
use hf_hub::api::tokio::Api;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnnoRagConfig::default();
    let api = Api::new()?;

    // ---- 1. Embedder ----
    println!("Warming embedder: {}", cfg.embed_model);
    warm_embedder(&api, &cfg.embed_model).await?;

    // ---- 2. NER (first available candidate) ----
    println!();
    println!("Warming anno NER (first available backend):");

    // Candidate list — favours the operator's explicit config field, then
    // falls back to the IDs anno's `StackedNER::default()` tries.
    let explicit = cfg.ner_warmup_model.as_deref();
    let defaults = [
        "onnx-community/gliner_small-v2.1",
        "knowledgator/gliner-pii-edge-v1.0",
        "numind/NuNER_Zero",
        "protectai/bert-base-NER-onnx",
    ];
    let candidates: Vec<&str> = explicit
        .into_iter()
        .chain(defaults.iter().copied())
        .collect();

    let mut warmed_any = false;
    for model_id in candidates {
        match warm_ner_model(&api, model_id).await {
            Ok(total_mb) => {
                println!("  {model_id} ... ok ({total_mb:.1} MiB)");
                warmed_any = true;
                break;
            }
            Err(e) => println!("  {model_id} ... skip ({e})"),
        }
    }
    if !warmed_any {
        eprintln!(
            "\nWARN: no NER model could be downloaded. anno will fall back to \
             pattern+heuristic and miss many French names. Check network / \
             HF Hub credentials and retry."
        );
    }

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

/// Try to download an anno NER model. Iterates a small set of likely
/// filenames per repo (config.json, tokenizer.json, plus one of three
/// possible weight extensions) and sums what landed. Returns total MiB
/// fetched. Errors if no file landed (the repo doesn't exist or is empty).
async fn warm_ner_model(api: &Api, model_id: &str) -> anyhow::Result<f64> {
    let repo = api.model(model_id.to_string());
    let mut total: u64 = 0;

    // Common metadata files.
    for file in ["config.json", "tokenizer.json", "tokenizer_config.json"] {
        if let Ok(path) = repo.get(file).await {
            if let Ok(meta) = std::fs::metadata(&path) {
                total += meta.len();
            }
        }
    }

    // One of these is the weights file.
    for file in ["model.onnx", "model.safetensors", "pytorch_model.bin"] {
        if let Ok(path) = repo.get(file).await {
            if let Ok(meta) = std::fs::metadata(&path) {
                total += meta.len();
                break; // Just one weights file is enough.
            }
        }
    }

    if total == 0 {
        anyhow::bail!("no candidate files downloaded");
    }
    Ok(total as f64 / 1024.0 / 1024.0)
}
