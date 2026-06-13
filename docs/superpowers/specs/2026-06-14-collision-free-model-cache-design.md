# Collision-Free Model Cache Design

## Goal

Replace the basename-only local cache key with the full HuggingFace repo ID as a two-level path, eliminating directory collisions between models that share the same trailing name across different organisations.

## Problem

Currently `ner_onnx_dir()`, `ner_candle_dir()`, and `embedder_dir()` return only the last segment of the repo ID:

```
SemplificaAI/gliner2-multi-v1-onnx  →  models/gliner2-multi-v1-onnx/
org-b/gliner2-multi-v1-onnx         →  models/gliner2-multi-v1-onnx/   ← collision
```

Two distinct organisations sharing a model name produce the same on-disk folder, allowing silent weight overwrites and audit-log mismatches.

---

## Section 1 — Config helper changes (`crates/anno-rag/src/config.rs`)

The three dir helpers return the full model ID, removing the `split('/')` step:

```rust
/// Full HF repo ID used as a two-level cache path.
/// Example: "SemplificaAI/gliner2-multi-v1-onnx"
pub fn ner_onnx_dir(&self) -> String {
    self.ner_model_id.clone()
}

/// Full candle model ID with "-candle" suffix.
/// Example: "fastino/gliner2-multi-v1" → "fastino/gliner2-multi-v1-candle"
pub fn ner_candle_dir(&self) -> String {
    format!("{}-candle", self.ner_candle_model_id)
}

/// Full embed model ID used as a two-level cache path.
/// Example: "OrdalieTech/Solon-embeddings-large-0.1"
pub fn embedder_dir(&self) -> String {
    self.embed_model.clone()
}
```

**No call-site changes required.** `PathBuf::join("SemplificaAI/gliner2-multi-v1-onnx")` produces a two-level path on all platforms including Windows (Rust normalises `/` on all targets). Format strings in `model_inventory.rs` (`"{ner_onnx_dir}/fp32_v2/..."`) become multi-level relative paths, which is correct when joined against `models_dir`.

Edge case: a model ID with no `/` (e.g. `"local-model"`) stays a single-level path — identical behaviour to today.

---

## Section 2 — Migration (`crates/anno-rag/src/model_cache.rs`)

New module with one public function:

```rust
pub fn migrate_legacy_cache(models_dir: &Path, cfg: &AnnoRagConfig)
```

Guarded by a `models_dir/.cache-v2` marker — no-op if the marker exists.

Migration logic per model (ONNX NER, Candle NER, embedder):

1. `canonical = models_dir.join(cfg.ner_onnx_dir())`  — new two-level path
2. `legacy    = models_dir.join(last_segment(&cfg.ner_model_id))`  — old basename, where `last_segment(id) = id.split('/').next_back().unwrap_or(id)`
3. If `canonical` does not exist AND `legacy` does:
   - `fs::create_dir_all(canonical.parent())`
   - `fs::rename(legacy, canonical)`
   - `tracing::info!("migrated model cache: {} → {}", legacy, canonical)`
4. Repeat for candle and embedder.
5. Write `models_dir/.cache-v2` to prevent re-running.

If `legacy == canonical` (model ID has no `/`), skip — nothing to rename.

**Called from two places:**
- `download_models::download()` — covers explicit `anno-rag download-models`
- `Pipeline::new()` — covers the common case where a user upgrades anno-rag and the pipeline starts with models already present in the legacy location; without this call, inventory would report models absent even though the files exist under the old basename dirs

---

## Section 3 — Tests

### `config.rs` — update existing dir-helper tests

| Assertion | Old expected | New expected |
|-----------|-------------|-------------|
| `ner_onnx_dir_is_last_segment_of_model_id` | `"gliner2-multi-v1-onnx"` | `"SemplificaAI/gliner2-multi-v1-onnx"` |
| `ner_candle_dir_appends_candle_suffix` | `"gliner2-multi-v1-candle"` | `"fastino/gliner2-multi-v1-candle"` |
| `embedder_dir_is_last_segment_of_embed_model` | `"Solon-embeddings-large-0.1"` | `"OrdalieTech/Solon-embeddings-large-0.1"` |
| `embedder_dir_no_slash_returns_whole_string` | `"local-model"` | `"local-model"` (unchanged) |

### `model_cache.rs` — new unit tests

- **`migrate_renames_legacy_dirs`** — creates three legacy basename dirs in a tempdir, calls `migrate_legacy_cache`, asserts new two-level dirs exist and legacy dirs are gone, asserts `.cache-v2` written.
- **`migrate_is_idempotent`** — `.cache-v2` already present → no rename attempted, no panic.
- **`migrate_skips_absent_legacy`** — no legacy dirs → no-op, `.cache-v2` written.
- **`migrate_partial`** — only ONNX legacy dir present → migrates it, skips candle/embedder, `.cache-v2` written.
- **`migrate_noop_when_canonical_already_exists`** — canonical dir already present → no rename.
- **`migrate_noop_for_model_id_without_slash`** — `ner_model_id = "local-model"` → legacy == canonical, no rename.

### Fixture updates

All tests that hardcode `"gliner2-multi-v1-onnx"`, `"gliner2-multi-v1-candle"`, or `"Solon-embeddings-large-0.1"` as paths are updated to use `cfg.ner_onnx_dir()`, `cfg.ner_candle_dir()`, `cfg.embedder_dir()` — consistent with the pattern established in the `feat/model-ids-configurable` PR.

---

## Affected files

| File | Change |
|------|--------|
| `crates/anno-rag/src/config.rs` | Simplify three dir helpers; update 4 unit tests |
| `crates/anno-rag/src/model_cache.rs` | New module: `migrate_legacy_cache` + 6 unit tests |
| `crates/anno-rag/src/lib.rs` | `pub mod model_cache;` |
| `crates/anno-rag/src/download_models.rs` | Call `migrate_legacy_cache` at top of `download()` |
| `crates/anno-rag/src/pipeline.rs` | Call `migrate_legacy_cache` at top of `Pipeline::new()` |
| `crates/anno-rag-mcp/src/lib.rs` | Update test fixtures |
| `crates/anno-rag-bin/src/main.rs` | Update test fixtures |

No changes to `detect.rs`, `model_inventory.rs`, or `setup_mcp.rs` — all use `cfg.*_dir()` via `PathBuf::join` which handles multi-level paths transparently.

---

## Migration path for existing users

1. User upgrades anno-rag.
2. On next run (any command that touches models), `migrate_legacy_cache` runs automatically.
3. Legacy dirs are renamed in-place — no re-download required.
4. `.cache-v2` marker written — subsequent runs skip the migration entirely.
5. If user has a custom `ANNO_MODELS_DIR` pointing to a read-only path, migration silently skips (rename fails gracefully) and logs a warning.
