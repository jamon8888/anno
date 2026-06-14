# Collision-Free Model Cache Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace basename-only local cache dirs with full HF repo ID two-level paths, eliminating collisions between models that share the same trailing name.

**Architecture:** Three config helpers return the full `org/model` string; `PathBuf::join` handles the slash on all platforms. A new `model_cache.rs` module provides `migrate_legacy_cache`, guarded by a `.cache-v2` marker, that renames old basename dirs to the new layout. Migration runs at `download()` and `Pipeline::new()`.

**Tech Stack:** Rust std (`std::fs`, `std::path::PathBuf`), `tracing`, `tempfile` (tests only)

---

## File Map

| File | Change |
|------|--------|
| `crates/anno-rag/src/model_cache.rs` | **Create** — `migrate_legacy_cache` + 6 unit tests |
| `crates/anno-rag/src/lib.rs` | Add `pub mod model_cache;` |
| `crates/anno-rag/src/config.rs` | Simplify 3 helpers; update 6 unit tests |
| `crates/anno-rag/src/download_models.rs` | Remove `split('/')` in `download_embedder`; call migration at top of `download()` |
| `crates/anno-rag/src/pipeline.rs` | Call migration at top of `Pipeline::new()` |
| `crates/anno-rag/src/detect.rs` | Update 3 test fixtures from hardcoded basename to `cfg.ner_onnx_dir()` |

---

### Task 1: Create `model_cache.rs` with `migrate_legacy_cache` (TDD)

**Files:**
- Create: `crates/anno-rag/src/model_cache.rs`
- Modify: `crates/anno-rag/src/lib.rs`

- [ ] **Step 1: Add the module declaration**

In `crates/anno-rag/src/lib.rs`, insert after `pub mod memory;` (line 41):

```rust
pub mod model_cache;
```

- [ ] **Step 2: Write the failing tests first**

Create `crates/anno-rag/src/model_cache.rs` with the test module only:

```rust
use crate::config::AnnoRagConfig;
use std::path::Path;

/// Renames legacy basename model dirs to the full `org/model` two-level layout.
///
/// Guarded by `models_dir/.cache-v2` — no-op if the marker already exists.
/// Errors during rename are logged as warnings and do not abort startup.
pub fn migrate_legacy_cache(models_dir: &Path, cfg: &AnnoRagConfig) {
    todo!("implement")
}

fn last_segment(model_id: &str) -> &str {
    model_id.split('/').next_back().unwrap_or(model_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn default_cfg() -> AnnoRagConfig {
        AnnoRagConfig::default()
    }

    #[test]
    fn migrate_renames_legacy_dirs() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Create legacy basename dirs
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_candle_model_id)
            .to_string() + "-candle")).unwrap();
        std::fs::create_dir_all(models.join(last_segment(&cfg.embed_model))).unwrap();

        migrate_legacy_cache(models, &cfg);

        // New two-level paths exist
        assert!(models.join(&cfg.ner_onnx_dir()).exists(), "onnx canonical missing");
        assert!(models.join(&cfg.ner_candle_dir()).exists(), "candle canonical missing");
        assert!(models.join(&cfg.embedder_dir()).exists(), "embedder canonical missing");

        // Legacy dirs gone
        assert!(!models.join(last_segment(&cfg.ner_model_id)).exists(), "legacy onnx still present");
        assert!(!models.join(last_segment(&cfg.embed_model)).exists(), "legacy embedder still present");

        // Marker written
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Pre-write marker
        std::fs::write(models.join(".cache-v2"), b"").unwrap();
        // Create a legacy dir that should NOT be renamed
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();

        migrate_legacy_cache(models, &cfg);

        // Legacy dir still present — migration was skipped
        assert!(models.join(last_segment(&cfg.ner_model_id)).exists());
    }

    #[test]
    fn migrate_skips_absent_legacy() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // No legacy dirs at all
        migrate_legacy_cache(models, &cfg);

        // Marker still written
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_partial_only_onnx_present() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Only ONNX legacy dir
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();

        migrate_legacy_cache(models, &cfg);

        assert!(models.join(&cfg.ner_onnx_dir()).exists(), "onnx canonical missing");
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_noop_when_canonical_already_exists() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let cfg = default_cfg();

        // Both canonical and legacy present (canonical wins — no rename)
        let canonical = models.join(&cfg.ner_onnx_dir());
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::create_dir_all(models.join(last_segment(&cfg.ner_model_id))).unwrap();

        migrate_legacy_cache(models, &cfg);

        // Canonical still there, legacy may or may not be (no clobber)
        assert!(canonical.exists());
        assert!(models.join(".cache-v2").exists());
    }

    #[test]
    fn migrate_noop_for_model_id_without_slash() {
        let dir = tempdir().unwrap();
        let models = dir.path();
        let mut cfg = default_cfg();
        cfg.ner_model_id = "local-model".to_string();

        // canonical == legacy for no-slash IDs — nothing to rename
        std::fs::create_dir_all(models.join("local-model")).unwrap();

        migrate_legacy_cache(models, &cfg);

        assert!(models.join("local-model").exists());
        assert!(models.join(".cache-v2").exists());
    }
}
```

- [ ] **Step 3: Run tests to confirm they fail**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: compile error `todo!("implement")` panics or tests fail.

- [ ] **Step 4: Implement `migrate_legacy_cache`**

Replace the `todo!` stub:

```rust
pub fn migrate_legacy_cache(models_dir: &Path, cfg: &AnnoRagConfig) {
    let marker = models_dir.join(".cache-v2");
    if marker.exists() {
        return;
    }

    let migrations = [
        (cfg.ner_onnx_dir(),    last_segment(&cfg.ner_model_id).to_string()),
        (cfg.ner_candle_dir(),  format!("{}-candle", last_segment(&cfg.ner_candle_model_id))),
        (cfg.embedder_dir(),    last_segment(&cfg.embed_model).to_string()),
    ];

    for (canonical_rel, legacy_rel) in &migrations {
        if canonical_rel == legacy_rel {
            continue; // model ID has no '/'; nothing to rename
        }
        let canonical = models_dir.join(canonical_rel);
        let legacy = models_dir.join(legacy_rel);
        if !canonical.exists() && legacy.exists() {
            if let Some(parent) = canonical.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("migrate_legacy_cache: create_dir_all {}: {e}", parent.display());
                    continue;
                }
            }
            match std::fs::rename(&legacy, &canonical) {
                Ok(()) => tracing::info!(
                    "migrated model cache: {} → {}",
                    legacy.display(),
                    canonical.display()
                ),
                Err(e) => tracing::warn!(
                    "migrate_legacy_cache: rename {} → {}: {e}",
                    legacy.display(),
                    canonical.display()
                ),
            }
        }
    }

    if let Err(e) = std::fs::write(&marker, b"") {
        tracing::warn!("migrate_legacy_cache: write marker {}: {e}", marker.display());
    }
}
```

- [ ] **Step 5: Run tests to confirm they pass**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all `model_cache::tests::*` tests pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/model_cache.rs crates/anno-rag/src/lib.rs
git commit -m "feat(cache): add migrate_legacy_cache to model_cache module"
```

---

### Task 2: Simplify config helpers to return full repo ID

**Files:**
- Modify: `crates/anno-rag/src/config.rs` (lines 1102–1138 for helpers, lines 1540–1587 for tests)

- [ ] **Step 1: Update the three helper functions**

Replace the current `ner_onnx_dir`, `ner_candle_dir`, `embedder_dir` implementations:

```rust
/// Full HF repo ID used as a two-level cache path.
///
/// Example: `"SemplificaAI/gliner2-multi-v1-onnx"` → `"SemplificaAI/gliner2-multi-v1-onnx"`
/// Edge case: `"local-model"` (no `/`) → `"local-model"` (single-level, unchanged)
#[must_use]
pub fn ner_onnx_dir(&self) -> String {
    self.ner_model_id.clone()
}

/// Full candle model ID with `"-candle"` appended.
///
/// Example: `"fastino/gliner2-multi-v1"` → `"fastino/gliner2-multi-v1-candle"`
#[must_use]
pub fn ner_candle_dir(&self) -> String {
    format!("{}-candle", self.ner_candle_model_id)
}

/// Full embed model ID used as a two-level cache path.
///
/// Example: `"OrdalieTech/Solon-embeddings-large-0.1"` → `"OrdalieTech/Solon-embeddings-large-0.1"`
#[must_use]
pub fn embedder_dir(&self) -> String {
    self.embed_model.clone()
}
```

- [ ] **Step 2: Update the seven unit tests in `config.rs`**

Find and replace the six affected tests (around lines 1540–1587):

```rust
#[test]
fn ner_onnx_dir_is_full_model_id() {
    let cfg = AnnoRagConfig::default();
    assert_eq!(cfg.ner_onnx_dir(), "SemplificaAI/gliner2-multi-v1-onnx");
}

#[test]
fn ner_candle_dir_is_full_model_id_with_candle_suffix() {
    let cfg = AnnoRagConfig::default();
    assert_eq!(cfg.ner_candle_dir(), "fastino/gliner2-multi-v1-candle");
}

#[test]
fn ner_onnx_dir_uses_custom_model_id() {
    let mut cfg = AnnoRagConfig::default();
    cfg.ner_model_id = "myorg/my-gliner-onnx".to_string();
    assert_eq!(cfg.ner_onnx_dir(), "myorg/my-gliner-onnx");
}

#[test]
fn ner_candle_dir_uses_custom_candle_model_id() {
    let mut cfg = AnnoRagConfig::default();
    cfg.ner_candle_model_id = "myorg/my-gliner-pt".to_string();
    assert_eq!(cfg.ner_candle_dir(), "myorg/my-gliner-pt-candle");
}

#[test]
fn embedder_dir_is_full_embed_model_id() {
    let cfg = AnnoRagConfig::default();
    assert_eq!(cfg.embedder_dir(), cfg.embed_model.clone());
}

#[test]
fn embedder_dir_uses_custom_embed_model() {
    let mut cfg = AnnoRagConfig::default();
    cfg.embed_model = "OrdalieTech/Solon-embeddings-large-0.1".to_string();
    assert_eq!(cfg.embedder_dir(), "OrdalieTech/Solon-embeddings-large-0.1");
}

#[test]
fn embedder_dir_no_slash_returns_whole_string() {
    let mut cfg = AnnoRagConfig::default();
    cfg.embed_model = "local-model".to_string();
    assert_eq!(cfg.embedder_dir(), "local-model");
}
```

- [ ] **Step 3: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all `config::tests::*` pass.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/config.rs
git commit -m "feat(config): dir helpers return full HF repo ID as two-level cache path"
```

---

### Task 3: Fix `download_embedder` to use the full model ID path

**Files:**
- Modify: `crates/anno-rag/src/download_models.rs` (line 40)

Currently `download_embedder` splits the model ID to get only the basename — this contradicts the new scheme.

- [ ] **Step 1: Remove the `split('/')` in `download_embedder`**

Replace lines 39–42:

```rust
// BEFORE
async fn download_embedder(models_dir: &Path, model_id: &str) -> Result<()> {
    let subdir = model_id.split('/').next_back().unwrap_or(model_id);
    let embed_dir = models_dir.join(subdir);
    tokio::fs::create_dir_all(&embed_dir).await?;
```

With:

```rust
// AFTER
async fn download_embedder(models_dir: &Path, model_id: &str) -> Result<()> {
    let embed_dir = models_dir.join(model_id);
    tokio::fs::create_dir_all(&embed_dir).await?;
```

`tokio::fs::create_dir_all` handles multi-level paths (`org/model`) correctly.

- [ ] **Step 2: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all `download_models::*` tests pass.

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag/src/download_models.rs
git commit -m "fix(download): use full model ID path in download_embedder (no basename split)"
```

---

### Task 4: Wire `migrate_legacy_cache` into `download()` and `Pipeline::new()`

**Files:**
- Modify: `crates/anno-rag/src/download_models.rs` (top of `download()`)
- Modify: `crates/anno-rag/src/pipeline.rs` (top of `Pipeline::new()`)

- [ ] **Step 1: Call migration in `download()`**

In `crates/anno-rag/src/download_models.rs`, add the import and call at the top of `download()`:

```rust
// At top of file, add to imports:
use crate::model_cache::migrate_legacy_cache;

// In pub async fn download(cfg: &AnnoRagConfig) -> Result<PathBuf> {
pub async fn download(cfg: &AnnoRagConfig) -> Result<PathBuf> {
    let models_dir = cfg.models_cache();
    migrate_legacy_cache(&models_dir, cfg);   // ← add this line
    download_embedder(&models_dir, &cfg.embed_model).await?;
    download_ner(&models_dir, &cfg.ner_model_id, &cfg.ner_onnx_dir()).await?;
    // ... rest unchanged
```

- [ ] **Step 2: Call migration in `Pipeline::new()`**

In `crates/anno-rag/src/pipeline.rs`, add after line 164 (`std::fs::create_dir_all(&cfg.data_dir)...`):

```rust
pub async fn new(cfg: AnnoRagConfig, vault_key: [u8; 32]) -> Result<Self> {
    std::fs::create_dir_all(&cfg.data_dir).map_err(Error::from)?;
    crate::model_cache::migrate_legacy_cache(&cfg.models_cache(), &cfg);  // ← add this line
    let vault = match Vault::open(&cfg.vault_path(), vault_key) {
    // ... rest unchanged
```

- [ ] **Step 3: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all tests pass, no compile errors.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/download_models.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(cache): wire migrate_legacy_cache into download() and Pipeline::new()"
```

---

### Task 5: Update `detect.rs` test fixtures

**Files:**
- Modify: `crates/anno-rag/src/detect.rs` (lines ~1199–1246)

Three tests hardcode `"gliner2-multi-v1-onnx"` as a path. With the new scheme, `cfg.ner_onnx_dir()` returns `"SemplificaAI/gliner2-multi-v1-onnx"`, so these need updating.

- [ ] **Step 1: Fix `anno_models_dir_missing_ner_dir_does_not_satisfy_local_fast_path`**

```rust
// BEFORE (around line 1211):
assert!(!model_root.join("gliner2-multi-v1-onnx").exists());

// AFTER:
let cfg = crate::config::AnnoRagConfig::default();
assert!(!model_root.join(cfg.ner_onnx_dir()).exists());
```

- [ ] **Step 2: Fix `anno_models_dir_local_path_entered_when_ner_dir_exists`**

```rust
// BEFORE (around line 1220):
let ner_dir = dir.path().join("gliner2-multi-v1-onnx");

// AFTER:
let cfg_inner = crate::config::AnnoRagConfig::default();
let ner_dir = dir.path().join(cfg_inner.ner_onnx_dir());
```

- [ ] **Step 3: Fix `default_models_cache_local_path_entered_when_ner_dir_exists`**

```rust
// BEFORE (around line 1245):
let ner_dir = cfg.models_cache().join("gliner2-multi-v1-onnx");

// AFTER:
let ner_dir = cfg.models_cache().join(cfg.ner_onnx_dir());
```

- [ ] **Step 4: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all `detect::tests::*` tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/detect.rs
git commit -m "fix(test): update detect.rs fixtures to use cfg.ner_onnx_dir() two-level path"
```

---

### Task 6: Final fmt + full check

**Files:** all changed files

- [ ] **Step 1: Run cargo fmt**

```powershell
cargo fmt --all
```

Expected: no output (already clean, or files re-formatted).

- [ ] **Step 2: Check fmt is clean**

```powershell
cargo fmt --all -- --check
```

Expected: exits 0 with no output.

- [ ] **Step 3: Run full anno-rag test suite**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all tests pass.

- [ ] **Step 4: Commit fmt if needed**

```powershell
git add -u
git commit -m "style: cargo fmt after collision-free cache refactor"
```

- [ ] **Step 5: Push branch**

```powershell
git push -u origin feat/collision-free-model-cache
```
