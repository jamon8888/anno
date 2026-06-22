//! Pre-download anno-rag model weights to a local directory.
//!
//! After running, set `ANNO_MODELS_DIR=<path>` so both loaders skip
//! the HuggingFace Hub network fetch on every process start.

use crate::{config::AnnoRagConfig, error::Result, model_cache::migrate_legacy_cache, Error};
use std::path::{Path, PathBuf};

/// The eight base names of GLiNER2-Fastino's ONNX graphs (fp32_v2 layout).
const NER_ONNX_BASES: &[&str] = &[
    "encoder",
    "token_gather",
    "span_rep",
    "schema_gather",
    "count_pred_argmax",
    "count_lstm_fixed",
    "scorer",
    "classifier",
];

/// Ordered HF-relative candidate paths for one ONNX graph `base`, preferring
/// `precision` ("fp16" or "fp32") and falling back to the other.
fn onnx_candidates(base: &str, precision: &str) -> Vec<String> {
    let fp16 = format!("fp16_v2/{base}_fp16.onnx");
    let fp32 = format!("fp32_v2/{base}_fp32.onnx");
    if precision == "fp32" {
        vec![fp32, fp16]
    } else {
        vec![fp16, fp32]
    }
}

/// Download both model families into `cfg.models_cache()` using the layout
/// that `Embedder::load` and `Detector::new` expect when `ANNO_MODELS_DIR`
/// is set.
///
/// Returns the path of the populated models directory.
///
/// # Errors
/// Returns [`Error::Embed`] / [`Error::Detect`] on HF network failure, or
/// [`Error::Io`] on filesystem errors.
pub async fn download(cfg: &AnnoRagConfig) -> Result<PathBuf> {
    let models_dir = cfg.models_cache();
    for (model_id, label) in [
        (cfg.embed_model.as_str(), "embed_model"),
        (cfg.ner_model_id.as_str(), "ner_model_id"),
        (cfg.ner_pii_model_id.as_str(), "ner_pii_model_id"),
        (cfg.ner_candle_model_id.as_str(), "ner_candle_model_id"),
    ] {
        if !crate::model_cache::is_valid_model_subpath(model_id) {
            return Err(Error::Embed(format!("unsafe {label} value: {model_id:?}")));
        }
    }
    migrate_legacy_cache(&models_dir, cfg);
    download_embedder(&models_dir, &cfg.embed_model).await?;
    // Legal NER model (generalist — used by legal_extract_*, detect_with_labels).
    download_ner(
        &models_dir,
        &cfg.ner_model_id,
        &cfg.ner_onnx_dir(),
        cfg.ner_onnx_precision.as_str(),
    )
    .await?;
    // PII NER model (specialized — used by detect() and the privacy pipeline).
    download_ner(
        &models_dir,
        &cfg.ner_pii_model_id,
        &cfg.ner_pii_onnx_dir(),
        "fp16",
    )
    .await?;
    #[cfg(any(feature = "gpu-metal", feature = "gliner2-candle-cpu"))]
    download_candle_ner(&models_dir, &cfg.ner_candle_model_id, &cfg.ner_candle_dir()).await?;
    #[cfg(feature = "vlm-ocr")]
    match cfg.vlm_backend.as_deref() {
        Some("vllm") => {
            let safetensors_id = cfg
                .vlm_safetensors_model_id
                .as_deref()
                .unwrap_or("lightonai/LightOnOCR-2-1B");
            if !crate::model_cache::is_valid_model_subpath(safetensors_id) {
                return Err(Error::Embed(format!(
                    "unsafe vlm_safetensors_model_id value: {safetensors_id:?}"
                )));
            }
            download_vlm_safetensors(&models_dir, safetensors_id).await?;
        }
        Some("local") => {
            let gguf_id = cfg
                .vlm_gguf_model_id
                .as_deref()
                .unwrap_or("Mungert/LightOnOCR-1B-1025-GGUF");
            if !crate::model_cache::is_valid_model_subpath(gguf_id) {
                return Err(Error::Embed(format!(
                    "unsafe vlm_gguf_model_id value: {gguf_id:?}"
                )));
            }
            download_vlm_gguf(&models_dir, gguf_id).await?;
        }
        _ => {} // None, "off", or unknown — skip VLM downloads
    }
    Ok(models_dir)
}

async fn download_embedder(models_dir: &Path, model_id: &str) -> Result<()> {
    let embed_dir = models_dir.join(model_id);
    tokio::fs::create_dir_all(&embed_dir).await?;

    let api =
        hf_hub::api::tokio::Api::new().map_err(|e| Error::Embed(format!("hf-hub init: {e}")))?;
    let repo = api.model(model_id.to_string());

    // config.json
    let src = repo
        .get("config.json")
        .await
        .map_err(|e| Error::Embed(format!("config.json fetch: {e}")))?;
    tokio::fs::copy(&src, embed_dir.join("config.json")).await?;
    println!("  embedder config.json    ... ok");

    // tokenizer.json
    let src = repo
        .get("tokenizer.json")
        .await
        .map_err(|e| Error::Embed(format!("tokenizer.json fetch: {e}")))?;
    tokio::fs::copy(&src, embed_dir.join("tokenizer.json")).await?;
    println!("  embedder tokenizer.json ... ok");

    // weights — model.safetensors preferred, pytorch_model.bin fallback
    let (src, dest_name) = match repo.get("model.safetensors").await {
        Ok(p) => (p, "model.safetensors"),
        Err(_) => {
            let p = repo
                .get("pytorch_model.bin")
                .await
                .map_err(|e| Error::Embed(format!("weights fetch: {e}")))?;
            (p, "pytorch_model.bin")
        }
    };
    let size_mb = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0;
    tokio::fs::copy(&src, embed_dir.join(dest_name)).await?;
    // safetensors is preferred; pytorch_model.bin is a defensive fallback.
    // NOTE: a raw .bin cannot be mmap-loaded as safetensors — the error is
    // surfaced at load time with a clear message.
    if dest_name == "pytorch_model.bin" {
        tokio::fs::copy(
            embed_dir.join("pytorch_model.bin"),
            embed_dir.join("model.safetensors"),
        )
        .await?;
    }
    println!("  embedder weights        ... ok ({size_mb:.0} MiB)");
    Ok(())
}

async fn download_ner(
    models_dir: &Path,
    model_id: &str,
    ner_dir_name: &str,
    precision: &str,
) -> Result<()> {
    let ner_dir = models_dir.join(ner_dir_name);
    tokio::fs::create_dir_all(&ner_dir).await?;

    // GLiNER2 uses the sync hf-hub API internally; run in spawn_blocking
    let ner_dir_clone = ner_dir.clone();
    let model_id = model_id.to_string();
    let precision = precision.to_string();
    tokio::task::spawn_blocking(move || download_ner_sync(&ner_dir_clone, &model_id, &precision))
        .await
        .map_err(|e| Error::Detect(format!("spawn_blocking panic: {e}")))?
}

#[cfg(any(feature = "gpu-metal", feature = "gliner2-candle-cpu"))]
async fn download_candle_ner(
    models_dir: &Path,
    model_id: &str,
    candle_dir_name: &str,
) -> Result<()> {
    let candle_dir = models_dir.join(candle_dir_name);
    tokio::fs::create_dir_all(&candle_dir).await?;
    let api =
        hf_hub::api::tokio::Api::new().map_err(|e| Error::Detect(format!("hf-hub init: {e}")))?;
    let repo = api.model(model_id.to_string());
    for file in [
        "tokenizer.json",
        "config.json",
        "encoder_config/config.json",
        "model.safetensors",
    ] {
        let src = repo
            .get(file)
            .await
            .map_err(|e| Error::Detect(format!("{file} fetch: {e}")))?;
        let dest = candle_dir.join(file);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&src, dest).await?;
    }
    println!("  Candle NER model        ... ok");
    Ok(())
}

fn download_ner_sync(ner_dir: &Path, model_id: &str, precision: &str) -> Result<()> {
    use hf_hub::api::sync::Api;

    let api = Api::new().map_err(|e| Error::Detect(format!("hf-hub init: {e}")))?;
    let repo = api.model(model_id.to_string());

    // Tokenizer — try precision-matching subdir first, then the other, then bare root.
    let (preferred_tok, fallback_tok) = if precision == "fp32" {
        ("fp32_v2/tokenizer.json", "fp16_v2/tokenizer.json")
    } else {
        ("fp16_v2/tokenizer.json", "fp32_v2/tokenizer.json")
    };
    let tokenizer_candidates = [preferred_tok, fallback_tok, "tokenizer.json"];
    let (tokenizer_src, tokenizer_rel) = tokenizer_candidates
        .iter()
        .find_map(|&rel| repo.get(rel).ok().map(|p| (p, rel)))
        .ok_or_else(|| Error::Detect("gliner2 tokenizer not found on HF hub".into()))?;

    // Walk up from the downloaded path to find the snapshot root
    let snapshot_dir = find_snapshot_dir(&tokenizer_src, tokenizer_rel)?;

    // config.json — optional, ignore failure
    let _ = repo
        .get("fp32_v2/config.json")
        .or_else(|_| repo.get("config.json"));

    // 8 ONNX files — preferred precision first, fallback to the other
    for base in NER_ONNX_BASES {
        let candidates = onnx_candidates(base, precision);
        candidates
            .iter()
            .find_map(|c| repo.get(c.as_str()).ok())
            .ok_or_else(|| Error::Detect(format!("gliner2 onnx graph '{base}' not found")))?;
    }

    // Mirror the snapshot dir to ner_dir preserving subdirectory structure
    mirror_dir(&snapshot_dir, ner_dir)
}

/// Walk up from `downloaded_file` by `depth` (number of '/' in `relative_hint`)
/// until we reach the snapshot root. Verifies a dtype subdir (fp32_v2/ etc.) exists.
fn find_snapshot_dir(downloaded_file: &Path, relative_hint: &str) -> Result<PathBuf> {
    let depth = relative_hint.matches('/').count();
    let mut dir = downloaded_file
        .parent()
        .ok_or_else(|| Error::Detect("downloaded file has no parent".into()))?;
    for _ in 0..depth {
        dir = dir
            .parent()
            .ok_or_else(|| Error::Detect("snapshot dir walk exceeded filesystem root".into()))?;
    }
    let snapshot_dir = dir.to_path_buf();
    let has_subdir = ["fp32_v2", "fp16_v2", "fp32", "fp16"]
        .iter()
        .any(|s| snapshot_dir.join(s).is_dir());
    if !has_subdir {
        return Err(Error::Detect(format!(
            "snapshot dir has no dtype subdir: {}",
            snapshot_dir.display()
        )));
    }
    Ok(snapshot_dir)
}

/// Recursively copy every file from `src_root` to `dest_root`,
/// preserving relative paths. Subdirectories are created as needed.
fn mirror_dir(src_root: &Path, dest_root: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src_root) {
        let entry = entry.map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
        let rel = entry
            .path()
            .strip_prefix(src_root)
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
        let dest = dest_root.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    println!("  NER model               ... ok (~500 MiB)");
    Ok(())
}

/// Default GGUF quantization filename for `Mungert/LightOnOCR-1B-1025-GGUF`.
///
/// To use a different quantization variant, set `vlm_gguf_model_id` to the
/// desired repo and override this constant by pointing `ANNO_RAG_VLM_GGUF_MODEL_ID`
/// at a repo that contains the preferred `.gguf` file. The file name itself
/// is not yet configurable; change this constant if the upstream repo renames it.
#[cfg(feature = "vlm-ocr")]
const DEFAULT_GGUF_FILENAME: &str = "LightOnOCR-1B-1025-Q4_K_M.gguf";

/// Download safetensors VLM model files (`config.json`, `tokenizer.json`,
/// `tokenizer_config.json`, `model.safetensors`) for serving via vLLM.
///
/// The model is **not** loaded in-process; this is a download-only step.
/// Files are placed under `models_dir/<model_id>/`.
#[cfg(feature = "vlm-ocr")]
async fn download_vlm_safetensors(models_dir: &Path, model_id: &str) -> crate::error::Result<()> {
    let dest_dir = models_dir.join(model_id);
    tokio::fs::create_dir_all(&dest_dir).await?;

    let api =
        hf_hub::api::tokio::Api::new().map_err(|e| Error::Embed(format!("hf-hub init: {e}")))?;
    let repo = api.model(model_id.to_string());

    // config.json
    let src = repo
        .get("config.json")
        .await
        .map_err(|e| Error::Embed(format!("vlm safetensors config.json fetch: {e}")))?;
    tokio::fs::copy(&src, dest_dir.join("config.json")).await?;

    // tokenizer.json
    let src = repo
        .get("tokenizer.json")
        .await
        .map_err(|e| Error::Embed(format!("vlm safetensors tokenizer.json fetch: {e}")))?;
    tokio::fs::copy(&src, dest_dir.join("tokenizer.json")).await?;

    // tokenizer_config.json (optional — not all repos have it; skip silently)
    if let Ok(src) = repo.get("tokenizer_config.json").await {
        tokio::fs::copy(&src, dest_dir.join("tokenizer_config.json")).await?;
    }

    // weights — model.safetensors preferred, pytorch_model.bin fallback
    let (src, dest_name) = match repo.get("model.safetensors").await {
        Ok(p) => (p, "model.safetensors"),
        Err(_) => {
            let p = repo
                .get("pytorch_model.bin")
                .await
                .map_err(|e| Error::Embed(format!("vlm safetensors weights fetch: {e}")))?;
            (p, "pytorch_model.bin")
        }
    };
    let size_mb = tokio::fs::metadata(&src)
        .await
        .ok()
        .map(|m| m.len())
        .unwrap_or(0) as f64
        / 1_048_576.0;
    tokio::fs::copy(&src, dest_dir.join(dest_name)).await?;
    println!("  VLM safetensors ({model_id}) ... ok ({size_mb:.0} MiB)");
    Ok(())
}

/// Download the GGUF VLM model file for serving via llama-server.
///
/// Downloads [`DEFAULT_GGUF_FILENAME`] from `model_id` (repo-relative path).
/// Files are placed under `models_dir/<model_id>/`.
#[cfg(feature = "vlm-ocr")]
async fn download_vlm_gguf(models_dir: &Path, model_id: &str) -> crate::error::Result<()> {
    let dest_dir = models_dir.join(model_id);
    tokio::fs::create_dir_all(&dest_dir).await?;

    let api =
        hf_hub::api::tokio::Api::new().map_err(|e| Error::Embed(format!("hf-hub init: {e}")))?;
    let repo = api.model(model_id.to_string());

    let src = repo
        .get(DEFAULT_GGUF_FILENAME)
        .await
        .map_err(|e| Error::Embed(format!("vlm gguf {DEFAULT_GGUF_FILENAME} fetch: {e}")))?;
    let size_mb = tokio::fs::metadata(&src)
        .await
        .ok()
        .map(|m| m.len())
        .unwrap_or(0) as f64
        / 1_048_576.0;
    tokio::fs::copy(&src, dest_dir.join(DEFAULT_GGUF_FILENAME)).await?;
    println!("  VLM GGUF ({model_id}/{DEFAULT_GGUF_FILENAME}) ... ok ({size_mb:.0} MiB)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnnoRagConfig;

    #[test]
    fn onnx_candidates_fp16_first_when_precision_fp16() {
        let c = onnx_candidates("encoder", "fp16");
        assert_eq!(c[0], "fp16_v2/encoder_fp16.onnx");
        assert_eq!(c[1], "fp32_v2/encoder_fp32.onnx");
    }

    #[test]
    fn onnx_candidates_fp32_first_when_precision_fp32() {
        let c = onnx_candidates("encoder", "fp32");
        assert_eq!(c[0], "fp32_v2/encoder_fp32.onnx");
        assert_eq!(c[1], "fp16_v2/encoder_fp16.onnx");
    }

    #[test]
    fn download_targets_models_cache_path() {
        let cfg = AnnoRagConfig::default();
        let models_dir = cfg.models_cache();
        // models_cache() = data_dir/models
        assert_eq!(models_dir, cfg.data_dir.join("models"));
        assert!(
            models_dir.ends_with("models"),
            "models_cache must end with 'models'"
        );
    }

    #[test]
    fn find_snapshot_dir_strips_one_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fp32 = dir.path().join("fp32_v2");
        std::fs::create_dir_all(&fp32).expect("create fp32_v2");
        let fake_file = fp32.join("tokenizer.json");
        std::fs::write(&fake_file, b"{}").expect("write");

        let result =
            find_snapshot_dir(&fake_file, "fp32_v2/tokenizer.json").expect("find snapshot dir");
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_snapshot_dir_strips_zero_levels_for_root_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Create a dtype subdir so the snapshot check passes
        std::fs::create_dir_all(dir.path().join("fp32_v2")).expect("mkdir");
        let fake_file = dir.path().join("tokenizer.json");
        std::fs::write(&fake_file, b"{}").expect("write");

        let result = find_snapshot_dir(&fake_file, "tokenizer.json").expect("find snapshot dir");
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_snapshot_dir_rejects_missing_subdir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fake_file = dir.path().join("tokenizer.json");
        std::fs::write(&fake_file, b"{}").expect("write");

        let result = find_snapshot_dir(&fake_file, "tokenizer.json");
        assert!(result.is_err(), "must error without dtype subdir");
        assert!(result.unwrap_err().to_string().contains("no dtype subdir"));
    }

    #[test]
    fn mirror_dir_copies_full_tree() {
        let src = tempfile::tempdir().expect("src");
        let dst = tempfile::tempdir().expect("dst");

        let sub = src.path().join("fp32_v2");
        std::fs::create_dir_all(&sub).expect("mkdir sub");
        std::fs::write(sub.join("encoder_fp32.onnx"), b"onnx").expect("write onnx");
        std::fs::write(src.path().join("tokenizer.json"), b"{}").expect("write tok");

        mirror_dir(src.path(), dst.path()).expect("mirror_dir");

        assert!(dst
            .path()
            .join("fp32_v2")
            .join("encoder_fp32.onnx")
            .exists());
        assert!(dst.path().join("tokenizer.json").exists());
    }

    #[test]
    fn mirror_dir_preserves_nested_structure() {
        let src = tempfile::tempdir().expect("src");
        let dst = tempfile::tempdir().expect("dst");

        std::fs::create_dir_all(src.path().join("a").join("b")).expect("mkdir");
        std::fs::write(src.path().join("a").join("b").join("f.bin"), b"x").expect("write");

        mirror_dir(src.path(), dst.path()).expect("mirror");
        assert!(dst.path().join("a").join("b").join("f.bin").exists());
    }
}
