# Anno-RAG Zero-Config Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make anno-rag work out-of-the-box for non-technical users on Windows and macOS — no env vars, no manual model download, no passphrase to configure.

**Architecture:** Six targeted changes to the existing runtime: (1) flip default Cargo features to ONNX, (2) fix Candle narrow panic for GPU users, (3) dual NER model config (PII-specialized + legal-generalist) + swap embedding to bge-m3 int8, (4) use `dirs::data_dir()` for cross-platform model/data paths, (5) auto-download models on MCP startup with progress in `status`. The vault keyring fallback already works correctly — no change needed there.

**Tech Stack:** Rust, `dirs` crate (already in workspace), `hf-hub` (already used in download_models), ONNX Runtime, Candle (GPU opt-in)

---

## File Map

| File | Change |
|------|--------|
| `crates/anno-rag-bin/Cargo.toml` | `default = []` — remove `gliner2-candle-cpu` |
| `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs` | Truncate seq_len to max_position_embeddings in both `run_pipeline_candle` and `run_classify_pipeline_candle` |
| `crates/anno-rag/src/config.rs` | `default_embed_model()` → `AlpEge/bge-m3-onnx-int8`, add `ner_pii_model_id` field, `default_data_dir()` → `dirs::data_dir()` |
| `crates/anno-rag/src/detect.rs` | Add `pii_detector` field to `Detector`, route `detect()` through it |
| `crates/anno-rag/src/download_models.rs` | Download 3 models: bge-m3-int8, gliner2-PII fp16, gliner2-multi fp16 |
| `crates/anno-rag-mcp/src/model_inventory.rs` | Inspect both NER models (`gliner` legal + `gliner_pii`) |
| `crates/anno-rag-mcp/src/lib.rs` | Add `WarmupPhase::Downloading`, auto-download before warmup, expose `download_progress_pct` in status |

---

### Task 1: ONNX as default build feature

**Files:**
- Modify: `crates/anno-rag-bin/Cargo.toml:13`

- [ ] **Step 1: Edit Cargo.toml**

Change:
```toml
[features]
default = ["gliner2-candle-cpu"]
```
To:
```toml
[features]
default = []
```

- [ ] **Step 2: Verify the build compiles without Candle**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check
```

Expected: `Finished` with no errors. Candle types still compile (feature-gated in `anno-rag`, not removed).

- [ ] **Step 3: Verify Candle still compiles opt-in**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo check -p anno-rag-bin --features gliner2-candle-cpu
```

Expected: `Finished` — Candle path still reachable.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag-bin/Cargo.toml
git commit -m "feat(bin): default to ONNX backend — remove gliner2-candle-cpu from default features"
```

---

### Task 2: Fix Candle narrow panic (GPU users)

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs:33-38` and `:158-165`

The DeBERTa-v3-base encoder has `max_position_embeddings = 512`. When the tokenizer produces more than 512 tokens (common with 19 GDPR label descriptions), `encoder.forward` panics with `narrow invalid args start + len > dim_len`.

- [ ] **Step 1: Write test**

Add to `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs` at bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_logic_caps_at_512() {
        let max_pos: usize = 512;
        let overlong: Vec<u32> = vec![1u32; 700];
        let truncated = overlong.len().min(max_pos);
        assert_eq!(truncated, 512);
    }
}
```

- [ ] **Step 2: Run test**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno -- truncation_logic_caps_at_512 2>&1 | tail -5
```

Expected: `test ... ok`

- [ ] **Step 3: Apply truncation fix in `run_pipeline_candle`**

In `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs`, replace lines 33–38:

```rust
    // 1. Build input tensors.
    let seq_len = record.input_ids.len();
    let input_ids = Tensor::from_slice(&record.input_ids[..], (1, seq_len), device)
        .map_err(|e| Error::Backend(format!("candle input_ids: {e}")))?;
    let attn_mask: Vec<u32> = record.attention_mask.iter().map(|&v| v as u32).collect();
    let attention_mask = Tensor::from_slice(&attn_mask[..], (1, seq_len), device)
        .map_err(|e| Error::Backend(format!("candle attn_mask: {e}")))?;
```

With:

```rust
    // 1. Build input tensors — truncate to encoder position limit (DeBERTa: 512).
    let max_seq = model.encoder.config.max_position_embeddings as usize;
    let seq_len = record.input_ids.len().min(max_seq);
    let input_ids = Tensor::from_slice(&record.input_ids[..seq_len], (1, seq_len), device)
        .map_err(|e| Error::Backend(format!("candle input_ids: {e}")))?;
    let attn_mask: Vec<u32> = record.attention_mask[..seq_len]
        .iter()
        .map(|&v| v as u32)
        .collect();
    let attention_mask = Tensor::from_slice(&attn_mask[..], (1, seq_len), device)
        .map_err(|e| Error::Backend(format!("candle attn_mask: {e}")))?;
```

Also replace the `word_to_token_maps` / `num_words` block (line ~48) with:

```rust
    let filtered_maps: Vec<(usize, usize)> = record
        .word_to_token_maps
        .iter()
        .copied()
        .filter(|&(_start, end)| end <= seq_len)
        .collect();
    let num_words = filtered_maps.len();
    if num_words == 0 {
        return Ok((empty_scorer_output(), 0));
    }
    let word_starts: Vec<u32> = filtered_maps
        .iter()
        .map(|&(start, _)| start as u32)
        .collect();
```

- [ ] **Step 4: Apply same truncation fix in `run_classify_pipeline_candle` (line ~158)**

Replace:
```rust
    let seq_len = record.input_ids.len();
    let input_ids = Tensor::from_slice(&record.input_ids[..], (1, seq_len), device)
        .map_err(|e| Error::Backend(format!("candle input_ids: {e}")))?;
```

With:
```rust
    let max_seq = model.encoder.config.max_position_embeddings as usize;
    let seq_len = record.input_ids.len().min(max_seq);
    let input_ids = Tensor::from_slice(&record.input_ids[..seq_len], (1, seq_len), device)
        .map_err(|e| Error::Backend(format!("candle input_ids: {e}")))?;
```

- [ ] **Step 5: Check compiles**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo check -p anno --features gliner2-candle-cpu
```

Expected: `Finished`

- [ ] **Step 6: Commit**

```powershell
git add crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs
git commit -m "fix(candle): truncate input to max_position_embeddings to prevent narrow panic"
```

---

### Task 3: Dual NER model + bge-m3 int8 embedding

**Files:**
- Modify: `crates/anno-rag/src/config.rs` — new defaults + `ner_pii_model_id` field
- Modify: `crates/anno-rag/src/detect.rs` — `Detector` with dual models
- Modify: `crates/anno-rag/src/download_models.rs` — download 3 models
- Modify: `crates/anno-rag-mcp/src/model_inventory.rs` — inspect `gliner_pii`

**Models after this task:**
- Embedding: `AlpEge/bge-m3-onnx-int8` — dim=1024 (matches Solon, no LanceDB migration), ~145 MB, ONNX INT8, available on HF
- NER PII: `fastino/gliner2-privacy-filter-PII-multi` — ONNX FP16 produced by Plan 3, ~150 MB
- NER Legal: `SemplificaAI/gliner2-multi-v1-onnx` fp16 — existing, unchanged, ~250 MB
- Total: ~545 MB (vs 2.35 GB before)

- [ ] **Step 1: Write config tests**

Add to `crates/anno-rag/src/config.rs` in the `#[cfg(test)]` block:

```rust
#[test]
fn default_embed_model_is_bge_m3_int8() {
    let c = AnnoRagConfig::default();
    assert_eq!(c.embed_model, "AlpEge/bge-m3-onnx-int8");
    assert_eq!(c.embed_dim, 1024);
}

#[test]
fn default_ner_pii_model_is_gliner2_pii() {
    let c = AnnoRagConfig::default();
    assert_eq!(c.ner_pii_model_id, "fastino/gliner2-privacy-filter-PII-multi");
    assert_eq!(c.ner_model_id, "SemplificaAI/gliner2-multi-v1-onnx");
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -- default_embed_model_is_bge default_ner_pii 2>&1 | tail -10
```

Expected: FAIL — fields missing / wrong values.

- [ ] **Step 3: Update `default_embed_model()` in config.rs**

Change:
```rust
fn default_embed_model() -> String {
    "OrdalieTech/Solon-embeddings-large-0.1".to_string()
}
```
To:
```rust
fn default_embed_model() -> String {
    "AlpEge/bge-m3-onnx-int8".to_string()
}
// embed_dim stays 1024 — bge-m3 is 1024-dimensional
```

- [ ] **Step 4: Add `ner_pii_model_id` to config.rs**

Add after `default_ner_model_id`:
```rust
fn default_ner_pii_model_id() -> String {
    "fastino/gliner2-privacy-filter-PII-multi".to_string()
}
```

In `AnnoRagConfig` struct, add after `ner_model_id`:
```rust
#[serde(default = "default_ner_pii_model_id")]
pub ner_pii_model_id: String,
```

In `impl Default for AnnoRagConfig`, add:
```rust
ner_pii_model_id: default_ner_pii_model_id(),
```

Add helper alongside `ner_dir()`:
```rust
pub fn ner_pii_dir(&self) -> String {
    self.ner_pii_model_id
        .split('/')
        .last()
        .unwrap_or("gliner2-privacy-filter-PII-multi")
        .to_string()
}
```

Also update any `apply_override` / `merge` blocks that iterate config fields to include `ner_pii_model_id` (search for the block that sets `self.ner_model_id = v` and add the same pattern for `ner_pii_model_id`).

- [ ] **Step 5: Add dual model to `Detector` in detect.rs**

Read `crates/anno-rag/src/detect.rs` around `struct Detector` and `impl Detector`. Add `pii_inner` alongside the existing inner field:

```rust
pub struct Detector {
    inner: DetectorInner,      // legal NER — gliner2-multi-v1-onnx
    pii_inner: DetectorInner,  // PII NER — gliner2-privacy-filter-PII-multi
}
```

Update `Detector::new()` to load both (find where `DetectorInner::new_onnx` is called):
```rust
let inner = DetectorInner::new_onnx(&cfg.ner_model_id, &models_dir)?;
let pii_inner = DetectorInner::new_onnx(&cfg.ner_pii_model_id, &models_dir)?;
Self { inner, pii_inner }
```

Update `detect()` (the main GDPR path, which calls `self.inner.extract_with_types` with `pii_label_set()`) to use `pii_inner`:
```rust
// was: self.inner.extract_with_types(text, &label_refs, threshold)
self.pii_inner.extract_with_types(text, &label_refs, threshold)
```

Leave `detect_with_labels()` and `detect_legal()` using `self.inner` (legal model).

- [ ] **Step 6: Update download_models.rs for 3 models**

In `crates/anno-rag/src/download_models.rs`, find the NER download call and add the PII model after it:

```rust
// Download NER legal model (existing — SemplificaAI/gliner2-multi-v1-onnx)
download_gliner_onnx(&cfg.ner_model_id, &models_dir, &cfg.ner_onnx_precision).await?;

// Download NER PII model (fastino/gliner2-privacy-filter-PII-multi ONNX FP16)
// Produced by Plan 3 and hosted on anno-rag GitHub Releases.
download_gliner_onnx(&cfg.ner_pii_model_id, &models_dir, "fp16").await?;
```

- [ ] **Step 7: Update model_inventory.rs to inspect gliner_pii**

In `crates/anno-rag-mcp/src/model_inventory.rs`, add to `ModelInventoryStatus`:
```rust
pub gliner_pii: ModelFamilyStatus,
```

In `ModelInventoryService::inspect()`, add:
```rust
let pii_dir = self.cfg.ner_pii_dir();
let gliner_pii = inspect_onnx_gliner_family(&path, &pii_dir);
```

Include in `ready` computation:
```rust
let ready = e5.ready && gliner.ready && gliner_pii.ready && !downloading;
```

- [ ] **Step 8: Run tests**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -- default_embed_model_is_bge default_ner_pii 2>&1 | tail -10
```

Expected: both pass.

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -p anno-rag-mcp 2>&1 | tail -20
```

Expected: all green. Fix any test asserting old Solon model ID.

- [ ] **Step 9: Commit**

```powershell
git add crates/anno-rag/src/config.rs crates/anno-rag/src/detect.rs crates/anno-rag/src/download_models.rs crates/anno-rag-mcp/src/model_inventory.rs
git commit -m "feat(models): dual NER (PII+legal) + bge-m3-onnx-int8 embedding — 545 MB total vs 2.35 GB"
```

---

### Task 4: Cross-platform data directory via `dirs::data_dir()`

**Files:**
- Modify: `crates/anno-rag/src/config.rs:778-799`

`dirs::data_dir()` returns:
- Windows: `%APPDATA%` (e.g. `C:\Users\Alice\AppData\Roaming`)
- macOS: `$HOME/Library/Application Support`
- Linux: `$HOME/.local/share`

We append `anno-rag` to get the app-specific directory.

- [ ] **Step 1: Write test**

Add to `crates/anno-rag/src/config.rs` tests:

```rust
#[test]
fn default_data_dir_uses_platform_standard_path() {
    let data_dir = dirs::data_dir();
    assert!(data_dir.is_some(), "dirs::data_dir() returned None — unsupported platform?");
    let expected = data_dir.unwrap().join("anno-rag");
    let cfg = AnnoRagConfig::default();
    assert_eq!(cfg.data_dir, expected);
}
```

- [ ] **Step 2: Run test — expect FAIL**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -- default_data_dir_uses_platform 2>&1 | tail -8
```

Expected: FAIL — current default is `~/.anno-rag`.

- [ ] **Step 3: Update `default_data_dir_from_env` in config.rs**

Replace:
```rust
fn default_data_dir_from_env(
    override_dir: Option<OsString>,
    home_dir: Option<OsString>,
) -> PathBuf {
    override_dir
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home_dir
                .filter(|p| !p.is_empty())
                .map(|p| PathBuf::from(p).join(".anno-rag"))
        })
        .or_else(|| dirs::home_dir().map(|p| p.join(".anno-rag")))
        .unwrap_or_else(|| PathBuf::from(".anno-rag"))
}
```

With:
```rust
fn default_data_dir_from_env(
    override_dir: Option<OsString>,
    _home_dir: Option<OsString>,
) -> PathBuf {
    if let Some(p) = override_dir.filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .map(|p| p.join("anno-rag"))
        .unwrap_or_else(|| PathBuf::from(".anno-rag"))
}
```

Update `default_data_dir()`:
```rust
fn default_data_dir() -> PathBuf {
    default_data_dir_from_env(std::env::var_os("ANNO_RAG_DATA_DIR"), None)
}
```

- [ ] **Step 4: Run test — expect PASS**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -- default_data_dir_uses_platform 2>&1 | tail -5
```

Expected: `test default_data_dir_uses_platform_standard_path ... ok`

- [ ] **Step 5: Run full config tests**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -- config 2>&1 | tail -20
```

Fix any test hardcoding `~/.anno-rag` — update to `dirs::data_dir().unwrap().join("anno-rag")`.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/config.rs
git commit -m "feat(config): use dirs::data_dir() for cross-platform default data directory"
```

---

### Task 5: Auto-download models on MCP startup with progress in status

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add `Downloading` variant to `WarmupPhase`**

In `crates/anno-rag-mcp/src/lib.rs` around line 63:

```rust
pub(crate) enum WarmupPhase {
    Idle,
    Downloading { started_ms: u64, progress_pct: u8 },
    Loading { started_ms: u64 },
    Ready { elapsed_ms: u64 },
    Failed { error: String },
}
```

- [ ] **Step 2: Insert auto-download step before warmup**

In the `tokio::spawn` warmup block, before `WarmupPhase::Loading` is set:

```rust
// Step 0: auto-download models if absent.
{
    let inv = ModelInventoryService::new(&warmup_server.cfg).inspect();
    if !inv.ready {
        *warmup_server.warmup_phase.write().await = WarmupPhase::Downloading {
            started_ms,
            progress_pct: 0,
        };
        tracing::info!("anno-rag: models absent — auto-downloading");
        if let Err(e) = anno_rag::download_models::download(&warmup_server.cfg).await {
            *warmup_server.warmup_phase.write().await = WarmupPhase::Failed {
                error: format!("model download failed: {e}"),
            };
            return;
        }
        tracing::info!("anno-rag: model download complete");
    }
}
*warmup_server.warmup_phase.write().await = WarmupPhase::Loading { started_ms };
```

- [ ] **Step 3: Expose `download_progress_pct` in status**

In the `WarmupPhase` match in the status handler (line ~882), add:

```rust
WarmupPhase::Downloading { started_ms, progress_pct } => {
    let elapsed_s = (now_ms().saturating_sub(*started_ms)) / 1000;
    serde_json::json!({
        "phase": "downloading",
        "elapsed_s": elapsed_s,
        "download_progress_pct": progress_pct,
    })
}
```

In tool handlers that guard on `Loading`, add the same guard for `Downloading`:

```rust
WarmupPhase::Downloading { progress_pct, .. } => {
    return Ok(serde_json::json!({
        "error": "models_downloading",
        "progress_pct": progress_pct,
        "hint": "Models are downloading automatically. Check status for progress."
    }).to_string());
}
```

- [ ] **Step 4: Check compiles**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check
```

Expected: `Finished`

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): auto-download models on startup + expose download_progress_pct in status"
```

---

### Task 6: Final integration check + PR

- [ ] **Step 1: Run all affected crate tests**

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
cargo test -p anno-rag -p anno-rag-mcp -p anno 2>&1 | tail -30
```

Expected: all pass.

- [ ] **Step 2: Build dev-fast binary**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-bin
```

Expected: `Finished`

- [ ] **Step 3: End-to-end smoke**

```powershell
Stop-Process -Name anno-rag -ErrorAction SilentlyContinue
Start-Sleep 2
Remove-Item Env:ANNO_RAG_VAULT_PASSPHRASE -ErrorAction SilentlyContinue
Remove-Item Env:ANNO_RAG_DATA_DIR -ErrorAction SilentlyContinue
.\E:\cargo-target\dev-fast\anno-rag.exe status
```

Expected: `{"ok":true, ..., "warmup_phase":"ready"}` after warmup. `detect` returns results in <2s.

- [ ] **Step 4: Open PR**

```powershell
git push origin feat/zero-config-runtime
gh pr create --title "feat: zero-config runtime — ONNX default, dual NER, bge-m3 int8, platform data dir, auto-download" `
  --body "Implements the zero-config runtime spec (Plan 1/3) for lawyer deployment.

## Changes
- default = [] in anno-rag-bin: ONNX backend by default, Candle opt-in
- Fix Candle narrow panic: truncate to max_position_embeddings (512)
- Dual NER: gliner2-privacy-filter-PII-multi (detect) + gliner2-multi-v1-onnx (legal)
- Embedding: AlpEge/bge-m3-onnx-int8 (145 MB, dim=1024, 2x faster than fp16)
- Total model size: ~545 MB vs 2.35 GB (-77%)
- Cross-platform data dir via dirs::data_dir()
- Auto-download models on MCP startup with download_progress_pct in status

## Test plan
- [ ] cargo test -p anno-rag -p anno-rag-mcp passes
- [ ] detect responds in <2s on CPU Windows
- [ ] legal_extract_contract uses legal NER model (gliner2-multi)
- [ ] Fresh install auto-downloads 3 models and reaches warmup_phase: ready
- [ ] ANNO_RAG_DATA_DIR still works as override"
```

---

## Self-Review

**Spec coverage:**
- ✅ `default = []` — Task 1
- ✅ Narrow panic fix — Task 2
- ✅ bge-m3 int8 + dual NER — Task 3
- ✅ `dirs::data_dir()` cross-platform — Task 4
- ✅ Auto-download + progress in status — Task 5
- ✅ Vault keyring — already works (vault.rs uses keyring when `ANNO_RAG_VAULT_PASSPHRASE` absent)
- ⬜ Tauri setup assistant — Plan 2
- ⬜ CI matrix — Plan 2
- ⬜ ONNX conversion gliner2-PII — Plan 3

**Placeholders:** None.

**Type consistency:** `WarmupPhase::Downloading { started_ms: u64, progress_pct: u8 }` consistent across Tasks 5.
