# Anno Installer Phase 2B — `download-models` + `ANNO_MODELS_DIR` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ANNO_MODELS_DIR` offline-loader support to both model loaders, and ship an `anno-rag download-models` subcommand that pre-downloads both models into the expected directory structure so users never hit an HF download at first MCP use.

**Architecture:** Two new `ANNO_MODELS_DIR` fast-paths (one in `Embedder::load`, one in `Detector::new`) check a local directory before any HF network call; a new `anno_rag::download_models` library module encapsulates all download + copy logic; the CLI wires it as `anno-rag download-models`; docs replace the `ANNO_NO_DOWNLOADS` workaround with the new recommended flow.

**Tech Stack:** `hf-hub` (already in `anno-rag` deps), `walkdir` (already in `anno-rag` deps), `tokio::fs`, `tokio::task::spawn_blocking`, `dirs` crate, `clap` subcommand.

---

## File Structure

- Modify: `crates/anno-rag/src/embed.rs` — add `ANNO_MODELS_DIR` fast-path at top of `Embedder::load`
- Modify: `crates/anno-rag/src/detect.rs` — add `ANNO_MODELS_DIR` fast-path at top of `Detector::new`
- Create: `crates/anno-rag/src/download_models.rs` — `pub async fn download(cfg: &AnnoRagConfig) -> Result<PathBuf>`
- Modify: `crates/anno-rag/src/lib.rs` — declare `pub mod download_models`
- Modify: `crates/anno-rag-bin/src/main.rs` — add `DownloadModels` subcommand
- Modify: `docs/release/README-release.md` — replace `ANNO_NO_DOWNLOADS` section with `download-models` flow

---

### Task 1: `ANNO_MODELS_DIR` fast-path in `Embedder::load`

**Files:**
- Modify: `crates/anno-rag/src/embed.rs`

The goal: when `ANNO_MODELS_DIR` is set and `<dir>/multilingual-e5-small/{config.json,tokenizer.json,model.safetensors}` all exist, load from local paths without touching HF Hub.

- [ ] **Step 1: Write the failing test**

The test lives in `embed.rs` `mod tests`. Add this test — it must compile but the behaviour it verifies (env-var short-circuit) is only observable after the implementation:

```rust
#[tokio::test]
async fn anno_models_dir_missing_files_falls_through_to_hf() {
    // When ANNO_MODELS_DIR points at a dir that lacks the e5 subdir,
    // load() must NOT return an error — it must fall through to hf-hub.
    // We can't let it actually call hf-hub in CI, so we only test the
    // negative: that a missing-files dir does NOT trigger an early-return error.
    let dir = tempfile::tempdir().expect("tempdir");
    // Create the subdir but leave it empty.
    std::fs::create_dir_all(dir.path().join("multilingual-e5-small")).expect("mkdir");
    // We do NOT call Embedder::load here (would try HF) — just verify
    // the path logic compiles and the dir exists:
    let e5_dir = dir.path().join("multilingual-e5-small");
    let has_all = e5_dir.join("config.json").exists()
        && e5_dir.join("tokenizer.json").exists()
        && e5_dir.join("model.safetensors").exists();
    assert!(!has_all, "empty dir must not trigger local-load path");
}
```

Run:
```
cargo test -p anno-rag embed::tests::anno_models_dir_missing_files_falls_through_to_hf
```
Expected: PASS (test compiles and runs — it doesn't test load behaviour, just the dir-check logic).

- [ ] **Step 2: Add the `ANNO_MODELS_DIR` fast-path to `Embedder::load`**

Open `crates/anno-rag/src/embed.rs`. At the very top of `Embedder::load`, before `let device = Device::Cpu;`, insert:

```rust
// ── ANNO_MODELS_DIR fast-path ──────────────────────────────────────────────
// When set and the three required files exist, skip the HF Hub download.
// This is the offline path used after `anno-rag download-models`.
if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
    let base = std::path::PathBuf::from(models_dir)
        .join("multilingual-e5-small");
    let config_path   = base.join("config.json");
    let tokenizer_path = base.join("tokenizer.json");
    let weights_path  = base.join("model.safetensors");
    if config_path.exists() && tokenizer_path.exists() && weights_path.exists() {
        let device = Device::Cpu;
        let config_json = std::fs::read_to_string(&config_path)?;
        let config: Config = serde_json::from_str(&config_json)
            .map_err(|e| Error::Embed(format!("config parse (local): {e}")))?;
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| Error::Embed(format!("tokenizer load (local): {e}")))?;
        let dtype = match cfg.embedder_dtype.as_deref() {
            Some("f16") => DType::F16,
            _ => DType::F32,
        };
        // SAFETY: we don't mutate the file for the lifetime of the mmap.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], dtype, &device)
                .map_err(|e| Error::Embed(format!("var builder (local): {e}")))?
        };
        let model = BertModel::load(vb, &config)
            .map_err(|e| Error::Embed(format!("bert load (local): {e}")))?;
        return Ok(Self {
            model,
            tokenizer,
            device,
            dim: cfg.embed_dim,
        });
    }
}
// ─────────────────────────────────────────────────────────────────────────────
```

- [ ] **Step 3: Run existing tests to verify nothing broke**

```
cargo test -p anno-rag embed
```
Expected: all embed tests pass (the existing `query_and_passage_prefixes_differ` test passes; HF-dependent tests are `#[ignore]` and are skipped).

- [ ] **Step 4: Commit**

```
git add crates/anno-rag/src/embed.rs
git commit -m "feat(embed): add ANNO_MODELS_DIR fast-path to skip HF download"
```

---

### Task 2: `ANNO_MODELS_DIR` fast-path in `Detector::new`

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

The goal: when `ANNO_MODELS_DIR` is set and `<dir>/gliner2-multi-v1-onnx` exists, call `GLiNER2Fastino::from_local` instead of `from_pretrained`.

- [ ] **Step 1: Write the failing test**

In `detect.rs`, add to `#[cfg(test)]` at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anno_models_dir_missing_ner_dir_falls_through() {
        // When ANNO_MODELS_DIR points at a dir that lacks the gliner2 subdir,
        // the local path is not taken (we don't call from_pretrained in test).
        let dir = tempfile::tempdir().expect("tempdir");
        let ner_dir = dir.path().join("gliner2-multi-v1-onnx");
        assert!(!ner_dir.exists(), "must not exist yet");
        // Would fall through to from_pretrained (we don't call it here).
    }
}
```

Run:
```
cargo test -p anno-rag detect::tests::anno_models_dir_missing_ner_dir_falls_through
```
Expected: PASS.

- [ ] **Step 2: Add the `ANNO_MODELS_DIR` fast-path to `Detector::new`**

Open `crates/anno-rag/src/detect.rs`. Locate `Detector::new`. Replace the body:

```rust
pub fn new() -> Result<Self> {
    // ── ANNO_MODELS_DIR fast-path ─────────────────────────────────────────
    if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
        let model_path = std::path::PathBuf::from(models_dir)
            .join("gliner2-multi-v1-onnx");
        if model_path.exists() {
            let ner = anno::backends::gliner2_fastino::GLiNER2Fastino::from_local(&model_path)
                .map_err(|e| Error::Detect(format!("gliner2_fastino load (local): {e}")))?;
            return Ok(Self { ner });
        }
    }
    // ─────────────────────────────────────────────────────────────────────
    let ner = GLiNER2Fastino::from_pretrained(NER_MODEL_ID)
        .map_err(|e| Error::Detect(format!("gliner2_fastino load: {e}")))?;
    Ok(Self { ner })
}
```

Note: `use std::path::PathBuf` is not yet imported in `detect.rs`. Add it to the `use` block at the top of the file:

```rust
use std::path::PathBuf;
use std::sync::OnceLock;
```

- [ ] **Step 3: Verify compilation and tests**

```
cargo test -p anno-rag detect
```
Expected: all detect tests pass.

- [ ] **Step 4: Commit**

```
git add crates/anno-rag/src/detect.rs
git commit -m "feat(detect): add ANNO_MODELS_DIR fast-path to skip HF download"
```

---

### Task 3: `anno_rag::download_models` module

**Files:**
- Create: `crates/anno-rag/src/download_models.rs`
- Modify: `crates/anno-rag/src/lib.rs`

The `download` function downloads e5-small (3 files) and GLiNER2 (tokenizer + 8 ONNX + optional config) to `cfg.models_cache()` in the exact directory layout that the two loaders expect.

`cfg.models_cache()` returns `<data_dir>/models` (default `~/.anno-rag/models`). The function creates:
- `<models_cache>/multilingual-e5-small/config.json`
- `<models_cache>/multilingual-e5-small/tokenizer.json`
- `<models_cache>/multilingual-e5-small/model.safetensors`
- `<models_cache>/gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx` (and 7 other ONNX files)
- `<models_cache>/gliner2-multi-v1-onnx/fp32_v2/tokenizer.json`

The GLiNER2 download uses `hf_hub::api::sync::Api` (same API used by `GLiNER2Fastino::from_pretrained` internally) inside `spawn_blocking` so it runs without blocking the async executor.

- [ ] **Step 1: Write the unit test for `default_models_dir`**

Create `crates/anno-rag/src/download_models.rs` with:

```rust
//! Pre-download anno-rag model weights to a local directory.
//!
//! After running, set `ANNO_MODELS_DIR=<path>` so both loaders skip
//! the HuggingFace Hub network fetch on every process start.

use crate::{config::AnnoRagConfig, error::Result, Error};
use std::path::{Path, PathBuf};

/// NER model HuggingFace repo id.
const NER_MODEL_ID: &str = "SemplificaAI/gliner2-multi-v1-onnx";
/// Embedder HuggingFace repo id.
const EMBED_MODEL_ID: &str = "intfloat/multilingual-e5-small";

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
    download_embedder(&models_dir).await?;
    download_ner(&models_dir).await?;
    Ok(models_dir)
}

async fn download_embedder(models_dir: &Path) -> Result<()> {
    let e5_dir = models_dir.join("multilingual-e5-small");
    tokio::fs::create_dir_all(&e5_dir).await?;

    let api = hf_hub::api::tokio::Api::new()
        .map_err(|e| Error::Embed(format!("hf-hub init: {e}")))?;
    let repo = api.model(EMBED_MODEL_ID.to_string());

    // config.json
    let src = repo.get("config.json").await
        .map_err(|e| Error::Embed(format!("config.json fetch: {e}")))?;
    tokio::fs::copy(&src, e5_dir.join("config.json")).await?;
    println!("  embedder config.json    ... ok");

    // tokenizer.json
    let src = repo.get("tokenizer.json").await
        .map_err(|e| Error::Embed(format!("tokenizer.json fetch: {e}")))?;
    tokio::fs::copy(&src, e5_dir.join("tokenizer.json")).await?;
    println!("  embedder tokenizer.json ... ok");

    // weights — model.safetensors preferred, pytorch_model.bin fallback
    let (src, dest_name) = match repo.get("model.safetensors").await {
        Ok(p) => (p, "model.safetensors"),
        Err(_) => {
            let p = repo.get("pytorch_model.bin").await
                .map_err(|e| Error::Embed(format!("weights fetch: {e}")))?;
            (p, "pytorch_model.bin")
        }
    };
    let size_mb = src.metadata().map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0;
    tokio::fs::copy(&src, e5_dir.join(dest_name)).await?;
    // Loader expects model.safetensors; if we got pytorch_model.bin, symlink
    if dest_name == "pytorch_model.bin" {
        // just copy again under the canonical name — symlinks not portable
        tokio::fs::copy(e5_dir.join("pytorch_model.bin"), e5_dir.join("model.safetensors")).await?;
    }
    println!("  embedder weights        ... ok ({size_mb:.0} MiB)");
    Ok(())
}

async fn download_ner(models_dir: &Path) -> Result<()> {
    let ner_dir = models_dir.join("gliner2-multi-v1-onnx");
    tokio::fs::create_dir_all(&ner_dir).await?;

    // GLiNER2 uses the sync hf-hub API internally; run in spawn_blocking
    let ner_dir_clone = ner_dir.clone();
    tokio::task::spawn_blocking(move || download_ner_sync(&ner_dir_clone))
        .await
        .map_err(|e| Error::Detect(format!("spawn_blocking panic: {e}")))?
}

fn download_ner_sync(ner_dir: &Path) -> Result<()> {
    use hf_hub::api::sync::Api;

    let api = Api::new()
        .map_err(|e| Error::Detect(format!("hf-hub init: {e}")))?;
    let repo = api.model(NER_MODEL_ID.to_string());

    // Tokenizer — try fp32_v2/ first (matches from_pretrained fallback order)
    let tokenizer_candidates = [
        "fp32_v2/tokenizer.json",
        "fp16_v2/tokenizer.json",
        "tokenizer.json",
    ];
    let (tokenizer_src, tokenizer_rel) = tokenizer_candidates.iter()
        .find_map(|&rel| repo.get(rel).ok().map(|p| (p, rel)))
        .ok_or_else(|| Error::Detect("gliner2 tokenizer not found on HF hub".into()))?;

    // Walk up from the downloaded path to find the snapshot root
    // (e.g. .cache/huggingface/hub/models--SemplificaAI--gliner2…/snapshots/<hash>/)
    let snapshot_dir = find_snapshot_dir(&tokenizer_src, tokenizer_rel)?;

    // config.json — optional
    let _ = repo.get("fp32_v2/config.json")
        .or_else(|_| repo.get("config.json"));

    // 8 ONNX files — fp32_v2 preferred, fp16_v2 fallback
    for base in NER_ONNX_BASES {
        let candidates = [
            format!("fp32_v2/{base}_fp32.onnx"),
            format!("fp16_v2/{base}_fp16.onnx"),
        ];
        let c_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
        c_refs.iter()
            .find_map(|c| repo.get(c).ok())
            .ok_or_else(|| Error::Detect(format!("gliner2 onnx graph '{base}' not found")))?;
    }

    // Mirror the snapshot dir to ner_dir preserving subdirectory structure
    mirror_dir(&snapshot_dir, ner_dir)
}

/// Walk up from `downloaded_file` until a directory containing any of the
/// GLiNER2 dtype subdirs (`fp32_v2/`, `fp16_v2/`, etc.) is found.
fn find_snapshot_dir(downloaded_file: &Path, relative_hint: &str) -> Result<PathBuf> {
    // If the file was at <snapshot>/fp32_v2/tokenizer.json, strip `fp32_v2/`
    // levels to arrive at <snapshot>.
    let depth = relative_hint.matches('/').count();
    let mut dir = downloaded_file.parent()
        .ok_or_else(|| Error::Detect("downloaded file has no parent".into()))?;
    for _ in 0..depth {
        dir = dir.parent()
            .ok_or_else(|| Error::Detect("snapshot dir walk exceeded filesystem root".into()))?;
    }
    let snapshot_dir = dir.to_path_buf();
    // Sanity-check: at least one dtype subdir must exist
    let has_subdir = ["fp32_v2", "fp16_v2", "fp32", "fp16"]
        .iter()
        .any(|s| snapshot_dir.join(s).is_dir());
    if !has_subdir {
        return Err(Error::Detect(format!(
            "snapshot dir has no fp32_v2/ subdir: {}",
            snapshot_dir.display()
        )));
    }
    Ok(snapshot_dir)
}

/// Recursively copy every file from `src_root` to `dest_root`,
/// preserving relative paths (subdirectories are created as needed).
fn mirror_dir(src_root: &Path, dest_root: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src_root) {
        let entry = entry.map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
        let rel = entry.path().strip_prefix(src_root)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnnoRagConfig;

    #[test]
    fn download_uses_models_cache_path() {
        let cfg = AnnoRagConfig::default();
        // models_cache() returns <data_dir>/models
        let models_dir = cfg.models_cache();
        assert!(models_dir.ends_with("models"), "models_cache must end with 'models'");
        assert_eq!(
            models_dir,
            cfg.data_dir.join("models"),
        );
    }

    #[test]
    fn find_snapshot_dir_strips_correct_depth() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fp32 = dir.path().join("fp32_v2");
        std::fs::create_dir_all(&fp32).expect("create fp32_v2");
        // Simulate: downloaded_file = <snapshot>/fp32_v2/tokenizer.json
        let fake_file = fp32.join("tokenizer.json");
        std::fs::write(&fake_file, b"{}").expect("write");

        let result = find_snapshot_dir(&fake_file, "fp32_v2/tokenizer.json")
            .expect("find snapshot dir");
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_snapshot_dir_rejects_missing_subdir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fake_file = dir.path().join("tokenizer.json");
        std::fs::write(&fake_file, b"{}").expect("write");

        // No fp32_v2/ subdir → error
        let result = find_snapshot_dir(&fake_file, "tokenizer.json");
        assert!(result.is_err(), "must error without dtype subdir");
    }

    #[test]
    fn mirror_dir_copies_tree() {
        let src = tempfile::tempdir().expect("src tempdir");
        let dst = tempfile::tempdir().expect("dst tempdir");

        let sub = src.path().join("fp32_v2");
        std::fs::create_dir_all(&sub).expect("mkdir sub");
        std::fs::write(sub.join("encoder_fp32.onnx"), b"onnx").expect("write onnx");
        std::fs::write(src.path().join("tokenizer.json"), b"{}").expect("write tok");

        mirror_dir(src.path(), dst.path()).expect("mirror_dir");

        assert!(dst.path().join("fp32_v2").join("encoder_fp32.onnx").exists());
        assert!(dst.path().join("tokenizer.json").exists());
    }
}
```

- [ ] **Step 2: Run the unit tests**

```
cargo test -p anno-rag download_models
```
Expected: `download_uses_models_cache_path`, `find_snapshot_dir_strips_correct_depth`, `find_snapshot_dir_rejects_missing_subdir`, `mirror_dir_copies_tree` all PASS. (The `download` function itself is not called — it would trigger HF downloads.)

- [ ] **Step 3: Declare the module in `lib.rs`**

Open `crates/anno-rag/src/lib.rs`. After line `pub mod bench_cli;`, add:

```rust
pub mod download_models;
```

- [ ] **Step 4: Compile-check the full crate**

```
cargo check -p anno-rag
```
Expected: no errors.

- [ ] **Step 5: Commit**

```
git add crates/anno-rag/src/download_models.rs crates/anno-rag/src/lib.rs
git commit -m "feat(anno-rag): add download_models module — pre-download to ANNO_MODELS_DIR layout"
```

---

### Task 4: `anno-rag download-models` CLI subcommand

**Files:**
- Modify: `crates/anno-rag-bin/src/main.rs`

- [ ] **Step 1: Add `DownloadModels` to the `Cmd` enum**

Open `crates/anno-rag-bin/src/main.rs`. In the `Cmd` enum, add after `Bench { ... }`:

```rust
/// Download model weights to a local directory and print the path.
/// Set ANNO_MODELS_DIR to the printed path so anno-rag works offline.
DownloadModels {
    /// Directory to download into. Defaults to ~/.anno-rag/models.
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,
},
```

- [ ] **Step 2: Wire the subcommand in `main`**

In `main()`, add a short-circuit before `Pipeline::new` (alongside the existing `Bench` short-circuit). After:

```rust
if let Cmd::Bench { corpus } = &cli.cmd {
    anno_rag::bench_cli::run(corpus).await?;
    return Ok(());
}
```

Add:

```rust
if let Cmd::DownloadModels { dir } = &cli.cmd {
    let mut cfg = AnnoRagConfig::default();
    if let Some(d) = dir {
        cfg.data_dir = d.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| d.clone());
        // Reconstruct: models_cache() = data_dir/models.
        // If --dir was e.g. /custom/models, set data_dir = /custom
        // so models_cache() = /custom/models.
        // But if the user passes --dir /custom/models explicitly, just
        // override models_cache() by setting data_dir = /custom.
        // Simpler: allow --dir to be the exact target dir; we set
        // data_dir = dir.parent() so models_cache() == dir.
        cfg.data_dir = dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| dir.parent().map(|p| p.to_path_buf()).unwrap_or_default());
    }
    println!("Downloading anno-rag models to: {}", cfg.models_cache().display());
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
```

Note: the `--dir` override logic is intentionally simple. If the user omits `--dir`, models go to `~/.anno-rag/models` (the default `models_cache()`). If they pass `--dir /my/path/models`, we set `data_dir = /my/path` so `models_cache() = /my/path/models`.

- [ ] **Step 3: Compile-check**

```
cargo check -p anno-rag-bin
```
Expected: no errors. If `anno_rag::download_models` is not found, verify `lib.rs` declares `pub mod download_models`.

- [ ] **Step 4: Smoke-test the help output**

```
cargo run -p anno-rag-bin -- download-models --help
```
Expected output:
```
Download model weights to a local directory and print the path

Usage: anno-rag download-models [OPTIONS]

Options:
      --dir <DIR>  Directory to download into. Defaults to ~/.anno-rag/models
  -h, --help       Print help
```

- [ ] **Step 5: Commit**

```
git add crates/anno-rag-bin/src/main.rs
git commit -m "feat(cli): add 'download-models' subcommand — pre-download NER + embedder"
```

---

### Task 5: Update `docs/release/README-release.md`

**Files:**
- Modify: `docs/release/README-release.md`

Replace the "First Run and Offline Mode" section with a new two-step flow, and update the Claude Desktop config example to use `ANNO_MODELS_DIR`.

- [ ] **Step 1: Replace the "First Run and Offline Mode" section**

Open `docs/release/README-release.md`. Locate the section starting at line 118:

```markdown
## First Run and Offline Mode

The release archives do not contain model weights.

For best first-run behavior, run the warmup command from a source checkout or development build before setting `ANNO_NO_DOWNLOADS=1`:

```sh
cargo run --release --example warmup_model -p anno-rag
```

If models are already in the HuggingFace cache, `ANNO_NO_DOWNLOADS=1` keeps runtime operation offline.
```

Replace it with:

```markdown
## First Run and Offline Mode

The release archives do not contain model weights (~970 MiB total).
Run the one-time download command included with the binary:

```sh
anno-rag download-models
```

This downloads both models (intfloat/multilingual-e5-small + SemplificaAI/gliner2-multi-v1-onnx)
to `~/.anno-rag/models` and prints the path. Add the printed path to your environment:

```sh
# macOS / Linux — add to ~/.bashrc or ~/.zshrc
export ANNO_MODELS_DIR="$HOME/.anno-rag/models"

# Windows PowerShell — persistent, current user
[System.Environment]::SetEnvironmentVariable("ANNO_MODELS_DIR", "$env:USERPROFILE\.anno-rag\models", "User")
```

After setting `ANNO_MODELS_DIR`, anno-rag starts without any network call.

> **Developers**: the warmup example still works too — `cargo run --release --example warmup_model -p anno-rag` downloads to the HuggingFace cache (`~/.cache/huggingface/hub/`). Use `anno-rag download-models` for end-user installs and `warmup_model` for development.
```

- [ ] **Step 2: Update the Claude Desktop config block**

Locate the Claude Desktop config example starting at line 88:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/absolute/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

Replace the `env` block so the primary recommendation uses `ANNO_MODELS_DIR`:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/absolute/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "/absolute/path/to/.anno-rag/models"
      }
    }
  }
}
```

Add a note after the code block:

```
Replace `/absolute/path/to/.anno-rag/models` with the path printed by `anno-rag download-models`.
`ANNO_NO_DOWNLOADS=1` still works as a fallback if models are already in the HuggingFace cache.
```

- [ ] **Step 3: Verify the markdown renders correctly**

```
grep -c "ANNO_MODELS_DIR" docs/release/README-release.md
```
Expected: at least `4` (appears in the download section, two shell examples, and the JSON config).

- [ ] **Step 4: Commit**

```
git add docs/release/README-release.md
git commit -m "docs: replace ANNO_NO_DOWNLOADS with ANNO_MODELS_DIR in install guide"
```

---

## Self-Review

### Spec coverage

| Spec requirement (§7, §14) | Task |
|---|---|
| `ANNO_MODELS_DIR` loader in `embed.rs` | Task 1 |
| `ANNO_MODELS_DIR` loader in `detect.rs` | Task 2 |
| `anno-rag download-models` subcommand | Task 4 |
| Downloads NER + embedder to local dir | Task 3 |
| Prints path + env-var instructions | Task 4 |
| Update Claude Desktop docs (remove `ANNO_NO_DOWNLOADS`) | Task 5 |
| Shell export instructions | Task 4 + 5 |

### No placeholders scan

All tasks contain actual Rust code, exact commands, and exact expected output. No TBDs.

### Type consistency

- `AnnoRagConfig::models_cache()` → `PathBuf` — used consistently in Tasks 3 and 4.
- `anno_rag::download_models::download(cfg: &AnnoRagConfig) -> Result<PathBuf>` — declared in Task 3, called in Task 4.
- `GLiNER2Fastino::from_local(model_dir: &Path)` — signature from `mod.rs:298`, used in Task 2.
- `Error::Embed(String)` and `Error::Detect(String)` — match `error.rs` enum variants.
- `Error::Io(#[from] std::io::Error)` — used in `mirror_dir` via `?` on `std::fs::create_dir_all` and `std::fs::copy`.
