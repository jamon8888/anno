# MCP Stabilization Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Anno MCP truthful and stable for selected client-folder ingestion, scoped search, corpus maintenance, model readiness, and vault initialization.

**Architecture:** Keep the heavy `Pipeline` lazy and reserved for operations that need anonymization, embeddings, GLiNER, or semantic search. Move MCP maintenance/status paths to small services that open only the stores they need: legal maintenance, model inventory, vault key status, and corpus registry helpers.

**Tech Stack:** Rust, Tokio, rmcp, LanceDB, rusqlite, keyring, Windows DPAPI via `windows-sys`, Python stdlib JSON-RPC smoke harness, PowerShell targeted dev loop.

---

## Source Context

- Spec: `docs/superpowers/specs/2026-06-03-mcp-stabilization-fixes-design.md`
- Review corrections applied in this plan:
  - Do not call `AnnoRagServer::pipeline()` from `forget/status/sources` maintenance paths because that helper is model-gated.
  - Mixed unified search needs an explicit response contract: `mode_used: "auto"` plus `scope_modes`.
  - Legal maintenance must open the main `anno_rag::store::Store`, not only `LegalStore`, because chunk counts and source-path deletion live there.
  - Model readiness must use the effective loader directory: `ANNO_MODELS_DIR` if set, otherwise `cfg.models_cache()`.
  - Vault status must distinguish `env_passphrase`, `keyring`, `dpapi_file`, and `missing`.

## File Map

- Modify `docs/superpowers/specs/2026-06-03-mcp-stabilization-fixes-design.md`
  - Align the spec with the code-review findings before implementation.
- Create `crates/anno-rag-mcp/src/model_inventory.rs`
  - File-level model readiness service for MCP status, download-models, and pipeline model gating.
- Create `crates/anno-rag-mcp/src/legal_maintenance.rs`
  - Store-only legal maintenance service for count/list/forget without model loading.
- Modify `crates/anno-rag-mcp/src/lib.rs`
  - Wire `ModelInventoryService`, `LegalMaintenanceService`, search auto mode, corpus get/health checks, status, sources, forget, and download-models.
- Modify `crates/anno-rag-mcp/src/corpus.rs`
  - Add corpus existence, get, and health DTO helpers around the existing `CorpusStore`.
- Modify `crates/anno-rag-mcp/src/health.rs`
  - Use shared vault key initialization/status service.
- Modify `crates/anno-rag/src/vault.rs`
  - Add shared vault key service/status and DPAPI fallback functions.
- Modify `crates/anno-rag/src/vault_admin.rs`
  - Make CLI `vault status` use the same service as MCP.
- Modify `crates/anno-rag/src/lib.rs`
  - Export the new vault key status types if `vault_admin` needs them from a separate module.
- Modify root `Cargo.toml`, `crates/anno-rag/Cargo.toml`, `Cargo.lock`
  - Add `windows-sys` as a direct workspace dependency for Windows DPAPI.
- Create `scripts/mcp_full_smoke.py`
  - JSON-RPC stdio harness that calls all exposed MCP tools against a temp data dir.
- Create `scripts/mcp-full-smoke.ps1`
  - PowerShell wrapper that sets env, picks the local exe, and runs the Python harness.

## Execution Rules

- Use targeted commands only. Do not run `cargo build --workspace` or release builds.
- Prefer:

```powershell
$env:CARGO_BUILD_JOBS='1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

- When a binary is needed:

```powershell
$env:CARGO_BUILD_JOBS='1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\mcp-iterate.ps1 -Build -Install
```

- Before each Rust command, check whether another build is already running:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

---

### Task 1: Patch The Spec To Match The Code Review

**Files:**
- Modify: `docs/superpowers/specs/2026-06-03-mcp-stabilization-fixes-design.md`

- [ ] **Step 1: Replace the `Forget` immediate patch wording**

Replace the sentence:

```markdown
- For `target` matching `legal_folder_*`, call `self.pipeline().await` before resolving/deleting in the immediate patch.
```

with:

```markdown
- For `target` matching `legal_folder_*`, use `LegalMaintenanceService` to resolve and delete the folder id without calling `self.pipeline().await`; `self.pipeline()` is model-gated and would make maintenance depend on downloaded models.
```

- [ ] **Step 2: Clarify the unified search response contract**

Under `### Search`, after the `scope="all"` bullet, insert:

```markdown
- For implicit mixed mode, return `mode_used="auto"` and a `scope_modes` object such as `{"knowledge":"fast","legal":"semantic"}` so callers can see that different backends used different execution modes.
```

- [ ] **Step 3: Clarify legal maintenance store ownership**

Under `### Corpus And Status`, append:

```markdown
- Legal chunk counts and folder deletion must use the main `anno_rag::store::Store`; `LegalStore` only holds legal enrichment rows and cannot report or delete all legal chunks by folder path.
```

- [ ] **Step 4: Clarify model inventory path**

Under `### Model Inventory Service`, append:

```markdown
- Model readiness must inspect the effective loader path: `ANNO_MODELS_DIR` when set, otherwise `cfg.models_cache()`.
```

- [ ] **Step 5: Clarify vault status states**

Under `### Vault Key Service`, append:

```markdown
- `vault status` and MCP health must report the active source as one of `env_passphrase`, `keyring`, `dpapi_file`, `kms_unimplemented`, or `missing`.
```

- [ ] **Step 6: Commit the spec alignment**

```powershell
git add docs\superpowers\specs\2026-06-03-mcp-stabilization-fixes-design.md
git commit -m "docs: align mcp stabilization spec with code review"
```

Expected: commit succeeds or reports there is nothing to commit if the text was already applied.

---

### Task 2: Add ModelInventoryService

**Files:**
- Create: `crates/anno-rag-mcp/src/model_inventory.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write the model inventory service and tests**

Create `crates/anno-rag-mcp/src/model_inventory.rs` with this content:

```rust
//! Model inventory checks for MCP status and download-models.

use anno_rag::config::AnnoRagConfig;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Aggregate readiness state for local model files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInventoryState {
    /// No required model files were found.
    Missing,
    /// A `.download-lock` sentinel exists.
    Downloading,
    /// Some required files exist, but at least one required file is missing.
    Partial,
    /// Every required file exists.
    Ready,
}

/// Per-model family readiness details.
#[derive(Debug, Clone, Serialize)]
pub struct ModelFamilyStatus {
    /// Family name.
    pub name: &'static str,
    /// Family root directory.
    pub path: String,
    /// Missing required relative files.
    pub missing_files: Vec<&'static str>,
    /// Whether every required file exists.
    pub ready: bool,
}

/// Full inventory returned by MCP status and download-models.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInventory {
    /// Effective model directory used by loaders.
    pub path: String,
    /// Whether `ANNO_MODELS_DIR` selected this path.
    pub from_env: bool,
    /// Aggregate readiness state.
    pub state: ModelInventoryState,
    /// True when all required files are present.
    pub ready: bool,
    /// True when the background download sentinel exists.
    pub downloading: bool,
    /// E5 embedding model status.
    pub e5: ModelFamilyStatus,
    /// GLiNER ONNX model status.
    pub gliner: ModelFamilyStatus,
}

/// Lightweight service. It only checks files; it never loads models.
#[derive(Debug, Clone)]
pub struct ModelInventoryService {
    cfg: AnnoRagConfig,
}

impl ModelInventoryService {
    /// Create a new service from Anno config.
    #[must_use]
    pub fn new(cfg: AnnoRagConfig) -> Self {
        Self { cfg }
    }

    /// Return the model inventory for the effective loader directory.
    #[must_use]
    pub fn inspect(&self) -> ModelInventory {
        let (models_dir, from_env) = effective_models_dir(&self.cfg);
        inspect_models_dir(&models_dir, from_env)
    }

    /// Return true only when every required model file exists.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.inspect().ready
    }
}

/// Determine the directory that model loaders will use.
#[must_use]
pub fn effective_models_dir(cfg: &AnnoRagConfig) -> (PathBuf, bool) {
    if let Some(path) = std::env::var_os("ANNO_MODELS_DIR") {
        return (PathBuf::from(path), true);
    }
    (cfg.models_cache(), false)
}

fn inspect_models_dir(models_dir: &Path, from_env: bool) -> ModelInventory {
    const E5_FILES: &[&str] = &[
        "multilingual-e5-small/config.json",
        "multilingual-e5-small/model.safetensors",
        "multilingual-e5-small/tokenizer.json",
    ];
    const GLINER_FILES: &[&str] = &[
        "gliner2-multi-v1-onnx/fp32_v2/classifier_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/count_lstm_fixed_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/count_pred_argmax_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/schema_gather_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/scorer_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/span_rep_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/token_gather_fp32.onnx",
        "gliner2-multi-v1-onnx/fp32_v2/tokenizer.json",
    ];

    let e5 = family_status("multilingual-e5-small", models_dir, E5_FILES);
    let gliner = family_status("gliner2-multi-v1-onnx", models_dir, GLINER_FILES);
    let downloading = models_dir.join(".download-lock").exists();
    let any_present = E5_FILES
        .iter()
        .chain(GLINER_FILES.iter())
        .any(|relative| models_dir.join(relative).exists());
    let ready = e5.ready && gliner.ready;
    let state = if ready {
        ModelInventoryState::Ready
    } else if downloading {
        ModelInventoryState::Downloading
    } else if any_present {
        ModelInventoryState::Partial
    } else {
        ModelInventoryState::Missing
    };

    ModelInventory {
        path: models_dir.display().to_string(),
        from_env,
        state,
        ready,
        downloading,
        e5,
        gliner,
    }
}

fn family_status(
    name: &'static str,
    models_dir: &Path,
    required_files: &'static [&'static str],
) -> ModelFamilyStatus {
    let missing_files = required_files
        .iter()
        .copied()
        .filter(|relative| !models_dir.join(relative).exists())
        .collect::<Vec<_>>();
    let ready = missing_files.is_empty();
    ModelFamilyStatus {
        name,
        path: models_dir.join(name).display().to_string(),
        missing_files,
        ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with_data_dir(path: &Path) -> AnnoRagConfig {
        AnnoRagConfig {
            data_dir: path.to_path_buf(),
            ..AnnoRagConfig::default()
        }
    }

    #[test]
    fn empty_model_dirs_are_partial_not_ready() {
        let dir = tempfile::tempdir().expect("temp dir");
        let models = dir.path().join("models");
        std::fs::create_dir_all(models.join("multilingual-e5-small")).expect("e5 dir");
        std::fs::create_dir_all(models.join("gliner2-multi-v1-onnx")).expect("gliner dir");
        let inventory = inspect_models_dir(&models, false);

        assert_eq!(inventory.state, ModelInventoryState::Missing);
        assert!(!inventory.ready);
        assert!(!inventory.e5.ready);
        assert!(!inventory.gliner.ready);
        assert!(inventory.e5.missing_files.contains(&"multilingual-e5-small/model.safetensors"));
        assert!(inventory.gliner.missing_files.contains(
            &"gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx"
        ));
    }

    #[test]
    fn full_required_files_are_ready() {
        let dir = tempfile::tempdir().expect("temp dir");
        let models = dir.path().join("models");
        for relative in [
            "multilingual-e5-small/config.json",
            "multilingual-e5-small/model.safetensors",
            "multilingual-e5-small/tokenizer.json",
            "gliner2-multi-v1-onnx/fp32_v2/classifier_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/count_lstm_fixed_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/count_pred_argmax_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/schema_gather_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/scorer_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/span_rep_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/token_gather_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/tokenizer.json",
        ] {
            let path = models.join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
            std::fs::write(path, b"x").expect("file");
        }

        let inventory = inspect_models_dir(&models, false);

        assert_eq!(inventory.state, ModelInventoryState::Ready);
        assert!(inventory.ready);
        assert!(inventory.e5.ready);
        assert!(inventory.gliner.ready);
    }

    #[test]
    fn effective_models_dir_prefers_env() {
        let dir = tempfile::tempdir().expect("temp dir");
        let env_dir = dir.path().join("env-models");
        let cfg = cfg_with_data_dir(dir.path());
        let saved = std::env::var_os("ANNO_MODELS_DIR");
        unsafe { std::env::set_var("ANNO_MODELS_DIR", &env_dir) };

        let (path, from_env) = effective_models_dir(&cfg);

        if let Some(value) = saved {
            unsafe { std::env::set_var("ANNO_MODELS_DIR", value) };
        } else {
            unsafe { std::env::remove_var("ANNO_MODELS_DIR") };
        }
        assert_eq!(path, env_dir);
        assert!(from_env);
    }
}
```

- [ ] **Step 2: Run the focused model inventory tests and see them pass**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp model_inventory -- --nocapture
```

Expected: all `model_inventory` tests pass.

- [ ] **Step 3: Wire the module and pipeline model gate**

In `crates/anno-rag-mcp/src/lib.rs`, add this module declaration near the existing modules:

```rust
pub mod model_inventory;
```

Replace the current `models_available` block inside `AnnoRagServer::pipeline()` with:

```rust
let inventory = crate::model_inventory::ModelInventoryService::new((*cfg).clone()).inspect();
if !inventory.ready {
    return Err(anno_rag::error::Error::Config(format!(
        "Models not ready at {} (state={:?}). Ask me to 'Set up anno-rag' \
         or run `anno-rag download-models` in a terminal, then restart the extension.",
        inventory.path, inventory.state
    )));
}
```

- [ ] **Step 4: Wire `status.models` to inventory without model loading**

In `status_impl_routing`, replace the `models` block with:

```rust
let inventory = crate::model_inventory::ModelInventoryService::new(self.cfg.as_ref().clone())
    .inspect();
let loaded = self.pipeline_arc();
let models = serde_json::json!({
    "inventory": inventory,
    "embedder_loaded": loaded.as_ref().is_some_and(|p| p.embedder_loaded()),
    "detector_loaded": loaded.as_ref().is_some_and(|p| p.detector_loaded()),
});
```

- [ ] **Step 5: Wire `download_models` to file-level inventory**

In `download_models`, replace the existing `e5_dir.exists() && gliner_dir.exists()` readiness check with:

```rust
let inventory = crate::model_inventory::ModelInventoryService::new(self.cfg.as_ref().clone())
    .inspect();
if inventory.ready {
    let wire = DownloadModelsResult {
        status: "already_present".into(),
        path: inventory.path.clone(),
        message: format!(
            "Models ready at {}. The effective model path is selected by {}.",
            inventory.path,
            if inventory.from_env { "ANNO_MODELS_DIR" } else { "the default cache" }
        ),
    };
    return serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"));
}
```

Keep the download target as `self.cfg.models_cache()` because `anno_rag::download_models::download()` writes to the default cache. If `inventory.from_env` is true and not ready, return:

```rust
let wire = DownloadModelsResult {
    status: format!("{:?}", inventory.state).to_lowercase(),
    path: inventory.path.clone(),
    message: format!(
        "ANNO_MODELS_DIR points to {}, but required files are missing. \
         Fix that directory or unset ANNO_MODELS_DIR before using download_models.",
        inventory.path
    ),
};
return serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"));
```

- [ ] **Step 6: Check the MCP crate**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo check -p anno-rag-mcp --lib
```

Expected: `anno-rag-mcp` compiles without loading or downloading models.

- [ ] **Step 7: Commit model inventory**

```powershell
git add crates\anno-rag-mcp\src\model_inventory.rs crates\anno-rag-mcp\src\lib.rs
git commit -m "fix: validate mcp model inventory"
```

---

### Task 3: Add LegalMaintenanceService

**Files:**
- Create: `crates/anno-rag-mcp/src/legal_maintenance.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Create the legal maintenance service**

Create `crates/anno-rag-mcp/src/legal_maintenance.rs`:

```rust
//! Legal maintenance helpers that do not load models or initialize Pipeline.

use anno_rag::{
    config::AnnoRagConfig,
    legal::{
        kg::{LanceGraphStore, LegalKnowledgeGraph},
        status::EnrichmentStatusStore,
        store::LegalStore,
    },
    store::Store,
};
use uuid::Uuid;

/// Lightweight legal maintenance service for MCP status/sources/forget.
pub struct LegalMaintenanceService {
    store: Store,
    legal_store: LegalStore,
    legal_kg: LanceGraphStore,
    enrichment_status: EnrichmentStatusStore,
}

impl LegalMaintenanceService {
    /// Open all legal stores needed for maintenance.
    pub async fn open(cfg: &AnnoRagConfig) -> anno_rag::Result<Self> {
        Ok(Self {
            store: Store::open(cfg).await?,
            legal_store: LegalStore::open(cfg).await?,
            legal_kg: LanceGraphStore::open(cfg).await?,
            enrichment_status: EnrichmentStatusStore::open(cfg).await?,
        })
    }

    /// Count all chunks in the main RAG store.
    pub async fn count_chunks(&self) -> anno_rag::Result<u64> {
        self.store.count_chunks().await
    }

    /// List distinct indexed legal folder paths from the main RAG store.
    pub async fn list_indexed_folder_paths(&self) -> anno_rag::Result<Vec<String>> {
        self.store.list_indexed_folder_paths().await
    }

    /// Resolve a stable MCP folder id back to a folder path.
    pub async fn resolve_folder_id(
        &self,
        id: &str,
        to_id: impl Fn(&str) -> String,
    ) -> anno_rag::Result<Option<String>> {
        let paths = self.list_indexed_folder_paths().await?;
        Ok(paths.into_iter().find(|path| to_id(path) == id))
    }

    /// Delete legal state whose `source_path` is inside `path`.
    pub async fn forget_folder_path(&self, path: &str) -> anno_rag::Result<u64> {
        let doc_ids = self.store.doc_ids_for_source_subtree(path).await?;
        self.delete_legal_auxiliary_rows(&doc_ids).await?;
        let report = self.store.delete_folder_rows(path).await?;
        Ok(report.removed_chunks)
    }

    /// Delete legal state for exact document ids.
    pub async fn forget_doc_ids(&self, doc_ids: &[Uuid]) -> anno_rag::Result<u64> {
        self.delete_legal_auxiliary_rows(doc_ids).await?;
        self.store.delete_doc_id_rows(doc_ids).await
    }

    async fn delete_legal_auxiliary_rows(&self, doc_ids: &[Uuid]) -> anno_rag::Result<()> {
        for doc_id in doc_ids {
            self.legal_store.delete_doc(*doc_id).await?;
            self.legal_kg.delete_doc(*doc_id).await?;
            self.enrichment_status.delete_doc(*doc_id).await?;
        }
        Ok(())
    }
}
```

- [ ] **Step 2: Add a lazy service field to `AnnoRagServer`**

In `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
mod legal_maintenance;
```

Add this field to `AnnoRagServer`:

```rust
legal_maintenance: Arc<OnceCell<Arc<crate::legal_maintenance::LegalMaintenanceService>>>,
```

Initialize it in both constructors:

```rust
legal_maintenance: Arc::new(OnceCell::new()),
```

Add this helper near `corpus()`:

```rust
async fn legal_maintenance(
    &self,
) -> anno_rag::Result<&crate::legal_maintenance::LegalMaintenanceService> {
    self.legal_maintenance
        .get_or_try_init(|| {
            let cfg = Arc::clone(&self.cfg);
            async move {
                crate::legal_maintenance::LegalMaintenanceService::open(&cfg)
                    .await
                    .map(Arc::new)
            }
        })
        .await
        .map(|arc| arc.as_ref())
}
```

- [ ] **Step 3: Change `sources_impl_routing` to use legal maintenance**

Replace the `if let Some(pipeline) = self.pipeline_arc()` block with:

```rust
if let Ok(legal) = self.legal_maintenance().await {
    if let Ok(paths) = legal.list_indexed_folder_paths().await {
        for path in paths {
            let id = legal_folder_id(&path);
            sources.push(serde_json::json!({
                "id": id,
                "kind": "legal_corpus",
                "label": id,
            }));
        }
    }
}
```

- [ ] **Step 4: Change `status_impl_routing` legal count**

Replace the legal block with:

```rust
let legal = match self.legal_maintenance().await {
    Ok(service) => match service.count_chunks().await {
        Ok(n) => serde_json::json!({ "chunks": n }),
        Err(e) => serde_json::json!({ "chunks": null, "error": e.to_string() }),
    },
    Err(e) => serde_json::json!({ "chunks": null, "error": e.to_string() }),
};
```

- [ ] **Step 5: Change legal folder id resolution**

Replace `resolve_legal_folder_id` with:

```rust
async fn resolve_legal_folder_id(&self, id: &str) -> Result<Option<String>, String> {
    let service = self.legal_maintenance().await.map_err(|e| e.to_string())?;
    service
        .resolve_folder_id(id, legal_folder_id)
        .await
        .map_err(|e| e.to_string())
}
```

Update call sites to remove the `pipeline` argument.

- [ ] **Step 6: Change `forget_impl_routing` to always attempt legal deletion**

In the `legal_folder_` branch, replace the pipeline-arc conditional with:

```rust
match self.resolve_legal_folder_id(&p.target).await {
    Ok(Some(path)) => match self.legal_maintenance().await {
        Ok(service) => match service.forget_folder_path(&path).await {
            Ok(removed) => legal_removed = removed,
            Err(e) => errors.push(format!("legal forget: {e}")),
        },
        Err(e) => errors.push(format!("legal maintenance: {e}")),
    },
    Ok(None) => {}
    Err(e) => errors.push(format!("legal resolve: {e}")),
}
```

In the path branch, replace the `pipeline_arc()` conditional with:

```rust
match self.legal_maintenance().await {
    Ok(service) => match service.forget_folder_path(&p.target).await {
        Ok(removed) => legal_removed = removed,
        Err(e) => errors.push(format!("legal forget: {e}")),
    },
    Err(e) => errors.push(format!("legal maintenance: {e}")),
}
```

- [ ] **Step 7: Change `forget_corpus` legal deletion**

Replace the `self.pipeline().await` deletion for `legal_doc_ids` with:

```rust
match self.legal_maintenance().await {
    Ok(service) => match service.forget_doc_ids(&legal_doc_ids).await {
        Ok(removed) => {
            legal_removed += removed;
            legal_deleted_by_doc_ids = true;
        }
        Err(e) => errors.push(format!("legal forget docs: {e}")),
    },
    Err(e) => errors.push(format!("legal maintenance: {e}")),
}
```

Also update any fallback call shaped like `resolve_legal_folder_id(pipeline, &binding.binding_id)` to `resolve_legal_folder_id(&binding.binding_id)`.

- [ ] **Step 8: Add a regression test for lazy forget not using the pipeline**

Append this test to `lazy_tests` in `crates/anno-rag-mcp/src/lib.rs`:

```rust
#[tokio::test]
async fn forget_path_attempts_legal_maintenance_without_pipeline() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..AnnoRagConfig::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

    let out = server
        .forget_impl_routing(ForgetParams {
            target: dir.path().join("client").display().to_string(),
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");

    assert_eq!(v["ok"], true);
    assert_eq!(v["removed"]["legal_chunks"], 0);
    assert!(server.pipeline_arc().is_none());
}
```

- [ ] **Step 9: Run focused tests**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp forget_path_attempts_legal_maintenance_without_pipeline sources_aggregates_knowledge_and_legal_corpora -- --nocapture
```

Expected: both tests pass and `pipeline_arc()` stays `None` for the lazy forget regression.

- [ ] **Step 10: Commit legal maintenance**

```powershell
git add crates\anno-rag-mcp\src\legal_maintenance.rs crates\anno-rag-mcp\src\lib.rs
git commit -m "fix: make mcp legal maintenance model-free"
```

---

### Task 4: Fix Unified Search Auto Mode

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add tests for the desired response contract**

Append these tests to `lazy_tests`:

```rust
#[tokio::test]
async fn search_legal_without_mode_uses_semantic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..AnnoRagConfig::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".into(),
            top_k: 5,
            mode: None,
            scope: Some("legal".into()),
            filters: None,
            corpus_id: None,
            allow_cross_corpus: true,
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");

    assert_eq!(v["mode_used"], "semantic");
    assert_eq!(v["scope_modes"]["legal"], "semantic");
    assert!(v["warnings"].as_array().expect("warnings").iter().all(|w| {
        !w.as_str().unwrap_or("").contains("legal scope skipped in fast mode")
    }));
}

#[tokio::test]
async fn search_all_without_mode_reports_auto_scope_modes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..AnnoRagConfig::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".into(),
            top_k: 5,
            mode: None,
            scope: Some("all".into()),
            filters: None,
            corpus_id: None,
            allow_cross_corpus: true,
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");

    assert_eq!(v["mode_used"], "auto");
    assert_eq!(v["scope_modes"]["knowledge"], "fast");
    assert_eq!(v["scope_modes"]["legal"], "semantic");
}

#[tokio::test]
async fn search_fast_legal_returns_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..AnnoRagConfig::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".into(),
            top_k: 5,
            mode: Some("fast".into()),
            scope: Some("legal".into()),
            filters: None,
            corpus_id: None,
            allow_cross_corpus: true,
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");

    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("legal scope requires semantic mode"));
}
```

- [ ] **Step 2: Add search execution planning helpers**

Replace `normalize_search_mode` with this enum and helper:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchBackendMode {
    Fast,
    Semantic,
    Skipped,
}

impl SearchBackendMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Semantic => "semantic",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
struct SearchExecutionPlan {
    mode_used: &'static str,
    knowledge: SearchBackendMode,
    legal: SearchBackendMode,
    explicit_fast_legal_error: bool,
}

fn search_execution_plan(
    mode: Option<String>,
    scope: &str,
    warnings: &mut Vec<String>,
) -> SearchExecutionPlan {
    match (mode.as_deref(), scope) {
        (None, "legal") => SearchExecutionPlan {
            mode_used: "semantic",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (None, "all") => SearchExecutionPlan {
            mode_used: "auto",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (None, "knowledge") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: false,
        },
        (Some("fast"), "legal") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: true,
        },
        (Some("fast"), "all") => {
            warnings.push(
                "legal scope skipped in fast mode (requires models). Use mode='semantic' to include legal results."
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "fast",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Skipped,
                explicit_fast_legal_error: false,
            }
        }
        (Some("fast"), "knowledge") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: false,
        },
        (Some("semantic"), "knowledge") => {
            warnings.push(
                "knowledge scope skipped in semantic mode (knowledge index currently supports fast mode only)"
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "semantic",
                knowledge: SearchBackendMode::Skipped,
                legal: SearchBackendMode::Skipped,
                explicit_fast_legal_error: false,
            }
        }
        (Some("semantic"), "legal") => SearchExecutionPlan {
            mode_used: "semantic",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (Some("semantic"), "all") => {
            warnings.push(
                "knowledge scope skipped in semantic mode (knowledge index currently supports fast mode only)"
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "semantic",
                knowledge: SearchBackendMode::Skipped,
                legal: SearchBackendMode::Semantic,
                explicit_fast_legal_error: false,
            }
        }
        (Some(other), _) => {
            warnings.push(format!(
                "unsupported search mode '{other}'; using implicit mode for scope='{scope}'"
            ));
            search_execution_plan(None, scope, warnings)
        }
    }
}
```

- [ ] **Step 3: Update `search_impl_routing` to use the execution plan**

After scope normalization, replace the `mode` assignment with:

```rust
let plan = search_execution_plan(p.mode.clone(), &scope, &mut warnings);
if plan.explicit_fast_legal_error {
    return serde_json::json!({
        "ok": false,
        "error": "legal scope requires semantic mode",
        "mode_used": plan.mode_used,
        "scope_used": scope,
        "scope_modes": {
            "knowledge": plan.knowledge.as_str(),
            "legal": plan.legal.as_str(),
        },
        "warnings": warnings,
    })
    .to_string();
}
```

In the knowledge block, replace the `mode == "semantic"` check with:

```rust
if plan.knowledge == SearchBackendMode::Fast {
```

and pass `mode: Some("fast".to_string())` to `KnowledgeSearchParams`.

In the legal block, replace the `mode == "fast"` check with:

```rust
if plan.legal == SearchBackendMode::Semantic {
```

The final response must include:

```rust
"mode_used": plan.mode_used,
"scope_modes": {
    "knowledge": plan.knowledge.as_str(),
    "legal": plan.legal.as_str(),
},
```

- [ ] **Step 4: Run focused search tests**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp search_legal_without_mode_uses_semantic search_all_without_mode_reports_auto_scope_modes search_fast_legal_returns_error search_fast_all_returns_legal_warning -- --nocapture
```

Expected: all listed search tests pass.

- [ ] **Step 5: Commit search fix**

```powershell
git add crates\anno-rag-mcp\src\lib.rs
git commit -m "fix: make unified mcp search mode explicit"
```

---

### Task 5: Fix Corpus Get And Health

**Files:**
- Modify: `crates/anno-rag-mcp/src/corpus.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add corpus service helpers**

In `crates/anno-rag-mcp/src/corpus.rs`, add this DTO:

```rust
/// MCP corpus health details.
#[derive(Debug, Clone, Serialize)]
pub struct CorpusHealthWire {
    /// Stable corpus id.
    pub corpus_id: String,
    /// Registry health field.
    pub health: String,
    /// Count of knowledge source bindings.
    pub knowledge_sources: usize,
    /// Count of legal document bindings.
    pub legal_documents: usize,
    /// Count of tabular review bindings.
    pub tabular_reviews: usize,
}
```

Add these methods to `impl CorpusService`:

```rust
/// Return whether a corpus exists.
pub fn corpus_exists(&self, corpus_id: CorpusId) -> anno_corpus_store::Result<bool> {
    self.store.corpus_exists(corpus_id)
}

/// Return one corpus wire row, or `None` if the id is unknown.
pub fn get(&self, corpus_id: CorpusId) -> anno_corpus_store::Result<Option<CorpusWire>> {
    let rows = self.list()?;
    Ok(rows.into_iter().find(|row| row.corpus_id == corpus_id.as_string()))
}

/// List all corpus rows.
pub fn list(&self) -> anno_corpus_store::Result<Vec<CorpusWire>> {
    Ok(self
        .store
        .list_corpora()?
        .into_iter()
        .map(|row| CorpusWire {
            corpus_id: row.corpus_id.as_string(),
            label: row.label_pseudo,
            health: row.health,
        })
        .collect())
}

/// Return health counts for one existing corpus.
pub fn health(&self, corpus_id: CorpusId) -> anno_corpus_store::Result<CorpusHealthWire> {
    let corpus = self
        .get(corpus_id)?
        .ok_or_else(|| anno_corpus_store::Error::Registry(format!(
            "unknown corpus {}",
            corpus_id.as_string()
        )))?;
    let bindings = self.store.bindings_for_corpus(corpus_id)?;
    let knowledge_sources = bindings
        .iter()
        .filter(|binding| binding.binding_kind == anno_corpus_core::CorpusBindingKind::KnowledgeSource)
        .count();
    let tabular_reviews = bindings
        .iter()
        .filter(|binding| binding.binding_kind == anno_corpus_core::CorpusBindingKind::TabularReview)
        .count();
    let legal_documents = self.store.document_ids_for_corpus(corpus_id, "legal")?.len();
    Ok(CorpusHealthWire {
        corpus_id: corpus.corpus_id,
        health: corpus.health,
        knowledge_sources,
        legal_documents,
        tabular_reviews,
    })
}
```

If `CorpusStore::list_corpora()` does not exist, add it in `crates/anno-corpus-store/src/store.rs` by querying `corpora` for `corpus_id`, `label_pseudo`, and `health`. Use the existing row types and migrations.

- [ ] **Step 2: Add unknown corpus tests**

Append to `lazy_tests`:

```rust
#[tokio::test]
async fn corpus_get_unknown_returns_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..AnnoRagConfig::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let id = uuid::Uuid::new_v4().to_string();

    let out = server.corpus_get(Parameters(CorpusIdParam { corpus_id: id })).await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");

    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("unknown corpus"));
}

#[tokio::test]
async fn corpus_health_unknown_returns_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..AnnoRagConfig::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let id = uuid::Uuid::new_v4().to_string();

    let out = server.corpus_health(Parameters(CorpusIdParam { corpus_id: id })).await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");

    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("unknown corpus"));
}
```

- [ ] **Step 3: Wire `corpus_get` and `corpus_health`**

In `lib.rs`, implement both handlers so they parse the UUID, check existence through `CorpusService`, and return JSON:

```rust
let parsed = match crate::corpus::parse_corpus_id(&p.corpus_id) {
    Ok(id) => id,
    Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
};
let service = match self.corpus().await {
    Ok(service) => service,
    Err(e) => return serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
};
match service.get(parsed) {
    Ok(Some(corpus)) => serde_json::json!({ "ok": true, "corpus": corpus }).to_string(),
    Ok(None) => serde_json::json!({
        "ok": false,
        "error": format!("unknown corpus {}", p.corpus_id),
    }).to_string(),
    Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
}
```

Use `service.health(parsed)` for `corpus_health` and return `{ "ok": true, "health": health }`.

- [ ] **Step 4: Run focused corpus tests**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp corpus_get_unknown_returns_error corpus_health_unknown_returns_error -- --nocapture
```

Expected: both tests pass.

- [ ] **Step 5: Commit corpus fixes**

```powershell
git add crates\anno-rag-mcp\src\corpus.rs crates\anno-rag-mcp\src\lib.rs crates\anno-corpus-store\src\store.rs
git commit -m "fix: validate mcp corpus lookups"
```

---

### Task 6: Add Shared Vault Key Status And Windows DPAPI Fallback

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/anno-rag/Cargo.toml`
- Modify: `crates/anno-rag/src/vault.rs`
- Modify: `crates/anno-rag/src/vault_admin.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Add direct Windows dependency**

In root `Cargo.toml` workspace dependencies, add:

```toml
windows-sys = { version = "0.61.2", features = ["Win32_Security_Cryptography", "Win32_System_Memory"] }
```

In `crates/anno-rag/Cargo.toml`, add:

```toml
windows-sys = { workspace = true }
```

- [ ] **Step 2: Add vault key status types**

In `crates/anno-rag/src/vault.rs`, add:

```rust
/// User-visible vault key source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultKeyStatusSource {
    /// Runtime passphrase from ANNO_RAG_VAULT_PASSPHRASE.
    EnvPassphrase,
    /// OS keyring entry.
    Keyring,
    /// Windows DPAPI protected current-user file.
    DpapiFile,
    /// KMS environment selected a source that is not implemented.
    KmsUnimplemented,
    /// No usable key source exists.
    Missing,
}

/// User-visible vault key status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultKeyStatus {
    /// Active source.
    pub source: VaultKeyStatusSource,
    /// Whether a key source is configured.
    pub present: bool,
    /// Whether the source can be used in the current process.
    pub usable: bool,
    /// Whether a fresh process can read the same source without another init call.
    pub persistent: bool,
    /// Human-readable remediation.
    pub message: String,
}
```

- [ ] **Step 3: Add DPAPI helpers**

Add these constants and functions in `vault.rs`:

```rust
const DPAPI_FILE_NAME: &str = "vault-key.dpapi";

fn dpapi_file_path() -> Result<std::path::PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| Error::Vault("cannot resolve local data directory".into()))?
        .join("anno-rag");
    std::fs::create_dir_all(&base)
        .map_err(|e| Error::Vault(format!("create dpapi key dir: {e}")))?;
    Ok(base.join(DPAPI_FILE_NAME))
}

#[cfg(windows)]
fn dpapi_protect(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, DATA_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };
    use windows_sys::Win32::System::Memory::LocalFree;

    let mut input = DATA_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &mut input,
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::Vault("CryptProtectData failed".into()));
    }
    let protected = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        let _ = LocalFree(output.pbData as _);
    }
    Ok(protected)
}

#[cfg(windows)]
fn dpapi_unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, DATA_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };
    use windows_sys::Win32::System::Memory::LocalFree;

    let mut input = DATA_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &mut input,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::Vault("CryptUnprotectData failed".into()));
    }
    let plaintext = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        let _ = LocalFree(output.pbData as _);
    }
    Ok(plaintext)
}

#[cfg(not(windows))]
fn dpapi_protect(_bytes: &[u8]) -> Result<Vec<u8>> {
    Err(Error::Vault("DPAPI fallback is only available on Windows".into()))
}

#[cfg(not(windows))]
fn dpapi_unprotect(_bytes: &[u8]) -> Result<Vec<u8>> {
    Err(Error::Vault("DPAPI fallback is only available on Windows".into()))
}

fn read_dpapi_key() -> Result<Option<[u8; 32]>> {
    let path = dpapi_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let protected = std::fs::read(&path)
        .map_err(|e| Error::Vault(format!("read dpapi key file: {e}")))?;
    let hex = String::from_utf8(dpapi_unprotect(&protected)?)
        .map_err(|e| Error::Vault(format!("dpapi key utf8: {e}")))?;
    parse_hex_key(&hex).map(Some)
}

fn write_dpapi_key(key: &[u8; 32]) -> Result<()> {
    let path = dpapi_file_path()?;
    let protected = dpapi_protect(hex_encode(key).as_bytes())?;
    std::fs::write(&path, protected)
        .map_err(|e| Error::Vault(format!("write dpapi key file: {e}")))?;
    let read_back = read_dpapi_key()?
        .ok_or_else(|| Error::Vault("dpapi key verification read no key".into()))?;
    if read_back != *key {
        return Err(Error::Vault("dpapi key verification read a different key".into()));
    }
    Ok(())
}
```

- [ ] **Step 4: Add shared status and initialization functions**

Add these public functions in `vault.rs`:

```rust
/// Return the effective vault key status without creating a new key.
#[must_use]
pub fn vault_key_status() -> VaultKeyStatus {
    let provider = std::env::var("ANNO_RAG_VAULT_KMS_PROVIDER").ok();
    let key_id = std::env::var("ANNO_RAG_VAULT_KMS_KEY_ID").ok();
    if provider.as_deref().is_some_and(|v| !v.is_empty())
        && key_id.as_deref().is_some_and(|v| !v.is_empty())
    {
        return VaultKeyStatus {
            source: VaultKeyStatusSource::KmsUnimplemented,
            present: true,
            usable: false,
            persistent: true,
            message: "KMS vault key source is configured but not implemented in this build".into(),
        };
    }
    if std::env::var("ANNO_RAG_VAULT_PASSPHRASE")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return VaultKeyStatus {
            source: VaultKeyStatusSource::EnvPassphrase,
            present: true,
            usable: true,
            persistent: false,
            message: "Using ANNO_RAG_VAULT_PASSPHRASE from the current process environment".into(),
        };
    }
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        if let Ok(hex) = entry.get_password() {
            if parse_hex_key(&hex).is_ok() {
                return VaultKeyStatus {
                    source: VaultKeyStatusSource::Keyring,
                    present: true,
                    usable: true,
                    persistent: true,
                    message: "Using OS keyring entry anno-rag:vault-key".into(),
                };
            }
        }
    }
    match read_dpapi_key() {
        Ok(Some(_)) => VaultKeyStatus {
            source: VaultKeyStatusSource::DpapiFile,
            present: true,
            usable: true,
            persistent: true,
            message: "Using Windows DPAPI protected key file".into(),
        },
        Ok(None) => VaultKeyStatus {
            source: VaultKeyStatusSource::Missing,
            present: false,
            usable: false,
            persistent: false,
            message: "No vault key found; run anno_init_vault or set ANNO_RAG_VAULT_PASSPHRASE".into(),
        },
        Err(e) => VaultKeyStatus {
            source: VaultKeyStatusSource::Missing,
            present: false,
            usable: false,
            persistent: false,
            message: format!("No usable vault key found: {e}"),
        },
    }
}

/// Store a passphrase-derived key using keyring, then DPAPI fallback on failure.
pub fn initialize_vault_key_from_passphrase(passphrase: &str) -> Result<VaultKeyStatus> {
    let key = derive_via_argon2(passphrase)?;
    match store_key_in_keyring(&key) {
        Ok(()) => Ok(vault_key_status()),
        Err(keyring_error) => {
            write_dpapi_key(&key)
                .map_err(|dpapi_error| Error::Vault(format!(
                    "keyring failed ({keyring_error}); dpapi fallback failed ({dpapi_error})"
                )))?;
            Ok(vault_key_status())
        }
    }
}

fn store_key_in_keyring(key: &[u8; 32]) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    entry
        .set_password(&hex_encode(key))
        .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
    let stored = entry
        .get_password()
        .map_err(|e| Error::Vault(format!("keyring set verification failed: {e}")))?;
    let stored_key = parse_hex_key(&stored)?;
    if stored_key != *key {
        return Err(Error::Vault("keyring verification read a different key".into()));
    }
    Ok(())
}
```

Change `derive_via_keyring()` so `NoEntry` first tries `read_dpapi_key()` before generating a fresh key:

```rust
Err(keyring::Error::NoEntry) => {
    if let Some(key) = read_dpapi_key()? {
        return Ok(key);
    }
    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    store_key_in_keyring(&key).or_else(|_| write_dpapi_key(&key))?;
    Ok(key)
}
```

Change `store_passphrase_derived_key_in_keyring` to call `initialize_vault_key_from_passphrase(passphrase).map(|_| ())`.

- [ ] **Step 5: Add vault tests**

In `vault.rs` tests, add:

```rust
#[test]
fn vault_key_status_prefers_env_passphrase() {
    let saved = std::env::var_os("ANNO_RAG_VAULT_PASSPHRASE");
    unsafe { std::env::set_var("ANNO_RAG_VAULT_PASSPHRASE", "test-passphrase") };

    let status = vault_key_status();

    if let Some(value) = saved {
        unsafe { std::env::set_var("ANNO_RAG_VAULT_PASSPHRASE", value) };
    } else {
        unsafe { std::env::remove_var("ANNO_RAG_VAULT_PASSPHRASE") };
    }
    assert_eq!(status.source, VaultKeyStatusSource::EnvPassphrase);
    assert!(status.present);
    assert!(status.usable);
}
```

- [ ] **Step 6: Wire MCP health and CLI status**

In `crates/anno-rag-mcp/src/health.rs`, replace `store_passphrase_derived_key_in_keyring(passphrase)` with:

```rust
match anno_rag::vault::initialize_vault_key_from_passphrase(passphrase) {
    Ok(status) if status.present && status.usable => InitVaultResult {
        ok: true,
        message: format!("Vault key initialized via {:?}", status.source),
    },
    Ok(status) => InitVaultResult {
        ok: false,
        message: status.message,
    },
    Err(e) => InitVaultResult {
        ok: false,
        message: e.to_string(),
    },
}
```

In `vault_admin.rs`, make `vault_status()` include:

```rust
let key_status = crate::vault::vault_key_status();
```

and serialize/report `key_source`, `key_present`, `key_usable`, `key_persistent`, and `key_message`. Keep the old `keyring_entry_present` field for compatibility, but derive it from `key_status.source == VaultKeyStatusSource::Keyring`.

- [ ] **Step 7: Run focused vault checks**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag vault_key_status_prefers_env_passphrase -- --nocapture
$env:CARGO_BUILD_JOBS='1'
cargo check -p anno-rag-bin
```

Expected: test passes and CLI binary crate checks.

- [ ] **Step 8: Verify fresh-process vault status**

```powershell
$exe = "$env:LOCALAPPDATA\anno-rag\anno-rag.exe"
& $exe vault status
```

Expected after successful `anno_init_vault`: output includes `key_present=true` and one of `key_source=keyring`, `key_source=dpapi_file`, or `key_source=env_passphrase`.

- [ ] **Step 9: Commit vault fix**

```powershell
git add Cargo.toml Cargo.lock crates\anno-rag\Cargo.toml crates\anno-rag\src\vault.rs crates\anno-rag\src\vault_admin.rs crates\anno-rag-mcp\src\health.rs
git commit -m "fix: share vault key status across cli and mcp"
```

---

### Task 7: Add Full MCP Smoke Harness

**Files:**
- Create: `scripts/mcp_full_smoke.py`
- Create: `scripts/mcp-full-smoke.ps1`

- [ ] **Step 1: Create Python JSON-RPC harness**

Create `scripts/mcp_full_smoke.py`:

```python
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time


def send(proc, msg_id, method, params=None):
    payload = {"jsonrpc": "2.0", "id": msg_id, "method": method}
    if params is not None:
        payload["params"] = params
    line = json.dumps(payload, separators=(",", ":")) + "\n"
    proc.stdin.write(line)
    proc.stdin.flush()
    while True:
        raw = proc.stdout.readline()
        if not raw:
            raise RuntimeError(f"process exited while waiting for {method}")
        data = json.loads(raw)
        if data.get("id") == msg_id:
            return data


def call_tool(proc, msg_id, name, arguments=None):
    return send(proc, msg_id, "tools/call", {
        "name": name,
        "arguments": arguments or {},
    })


def tool_text(response):
    result = response.get("result", {})
    content = result.get("content", [])
    if not content:
        return ""
    return content[0].get("text", "")


def parse_json_text(response):
    text = tool_text(response)
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text}


def write_fixture(root):
    root.mkdir(parents=True, exist_ok=True)
    (root / "client-note.txt").write_text(
        "Client ACME. Donnees brutes client. Contact jean@example.com.",
        encoding="utf-8",
    )
    (root / "contract.txt").write_text(
        "Contrat de prestation. Paiement sous 30 jours. Clause de confidentialite.",
        encoding="utf-8",
    )
    anon = root / "anon"
    anon.mkdir()
    (anon / "generated.anon.md").write_text("generated output must be ignored", encoding="utf-8")


def main():
    exe = pathlib.Path(os.environ["ANNO_RAG_EXE"])
    models = pathlib.Path(os.environ["ANNO_MODELS_DIR"])
    data_dir = pathlib.Path(tempfile.mkdtemp(prefix="anno-mcp-smoke-data-"))
    fixture = pathlib.Path(tempfile.mkdtemp(prefix="anno-mcp-smoke-fixture-"))
    write_fixture(fixture)

    env = os.environ.copy()
    env["ANNO_RAG_DATA_DIR"] = str(data_dir)
    env["ANNO_MODELS_DIR"] = str(models)
    env.setdefault("ANNO_RAG_VAULT_PASSPHRASE", "anno-local-smoke-passphrase")

    proc = subprocess.Popen(
        [str(exe), "mcp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        env=env,
    )
    msg = 1
    results = []
    try:
        init = send(proc, msg, "initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "anno-smoke", "version": "1.0"},
        })
        msg += 1
        results.append(("initialize", "pass", init))
        listed = send(proc, msg, "tools/list")
        msg += 1
        tools = [tool["name"] for tool in listed["result"]["tools"]]
        results.append(("tools/list", "pass", {"count": len(tools), "tools": tools}))

        calls = [
            ("anno_health", {}),
            ("status", {}),
            ("download_models", {}),
            ("anno_init_vault", {"passphrase": "anno-local-smoke-passphrase"}),
            ("index", {"path": str(fixture), "profile": "all"}),
            ("corpus_list", {}),
            ("sources", {}),
            ("knowledge_status", {}),
            ("knowledge_sources", {}),
            ("knowledge_search", {"query": "donnees brutes client", "allow_cross_corpus": True}),
            ("legal_search", {"query": "paiement confidentialite", "allow_cross_corpus": True}),
            ("search", {"query": "paiement confidentialite", "scope": "legal", "allow_cross_corpus": True}),
            ("search", {"query": "client contrat", "scope": "all", "allow_cross_corpus": True}),
            ("memory_save", {"text": "Preference client: reponse concise", "kind": "preference"}),
            ("memory_list", {}),
            ("memory_recall", {"query": "preference client"}),
            ("memory_graph_recall", {"entity": "client"}),
            ("memory_invalidate", {"id": "00000000-0000-0000-0000-000000000000"}),
            ("memory_forget", {"query": "preference client", "limit": 1}),
            ("review_create", {"title": "Smoke review", "corpus_id": None}),
            ("review_export", {"review_id": "00000000-0000-0000-0000-000000000000"}),
            ("vault_stats", {}),
            ("forget", {"target": str(fixture)}),
            ("status", {}),
        ]
        seen_review_id = None
        for name, args in calls:
            if name == "review_export" and seen_review_id:
                args = {"review_id": seen_review_id}
            response = call_tool(proc, msg, name, args)
            msg += 1
            parsed = parse_json_text(response)
            if name == "review_create" and isinstance(parsed, dict):
                seen_review_id = parsed.get("review_id")
            status = "pass"
            if "error" in response:
                status = "jsonrpc_error"
            elif isinstance(parsed, dict) and parsed.get("ok") is False:
                status = "contextual_error"
            results.append((name, status, parsed))

        summary = {
            "exe": str(exe),
            "data_dir": str(data_dir),
            "fixture": str(fixture),
            "models": str(models),
            "tool_count": len(tools),
            "calls": len(results),
            "failures": [r for r in results if r[1] in ("jsonrpc_error",)],
            "contextual_errors": [r for r in results if r[1] == "contextual_error"],
        }
        print(json.dumps(summary, indent=2, ensure_ascii=False))
        return 1 if summary["failures"] else 0
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Create PowerShell wrapper**

Create `scripts/mcp-full-smoke.ps1`:

```powershell
param(
  [string]$Exe = "$env:LOCALAPPDATA\anno-rag\anno-rag.exe",
  [string]$ModelsDir = "$env:USERPROFILE\.anno-rag\models"
)

$ErrorActionPreference = "Stop"
if (-not (Test-Path -LiteralPath $Exe)) {
  throw "anno-rag exe not found: $Exe"
}
if (-not (Test-Path -LiteralPath $ModelsDir)) {
  throw "models dir not found: $ModelsDir"
}

$env:ANNO_RAG_EXE = (Resolve-Path -LiteralPath $Exe).Path
$env:ANNO_MODELS_DIR = (Resolve-Path -LiteralPath $ModelsDir).Path
python scripts\mcp_full_smoke.py
```

- [ ] **Step 3: Run the harness after a targeted install build**

```powershell
$env:CARGO_BUILD_JOBS='1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\mcp-iterate.ps1 -Build -Install
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\mcp-full-smoke.ps1
```

Expected: output JSON reports `failures: []`. `contextual_errors` may include LLM/keyring dependent calls, but must not include `index`, `status`, `search`, `legal_search`, `knowledge_search`, `corpus_list`, or `forget`.

- [ ] **Step 4: Commit smoke harness**

```powershell
git add scripts\mcp_full_smoke.py scripts\mcp-full-smoke.ps1
git commit -m "test: add full mcp smoke harness"
```

---

### Task 8: Final Verification

**Files:**
- All files touched in Tasks 1-7.

- [ ] **Step 1: Run targeted Rust tests**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp model_inventory -- --nocapture
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp search_legal_without_mode_uses_semantic search_all_without_mode_reports_auto_scope_modes search_fast_legal_returns_error -- --nocapture
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag-mcp corpus_get_unknown_returns_error corpus_health_unknown_returns_error forget_path_attempts_legal_maintenance_without_pipeline -- --nocapture
$env:CARGO_BUILD_JOBS='1'
cargo test -p anno-rag vault_key_status_prefers_env_passphrase -- --nocapture
```

Expected: all targeted tests pass.

- [ ] **Step 2: Run targeted checks**

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo check -p anno-rag-mcp --lib
$env:CARGO_BUILD_JOBS='1'
cargo check -p anno-rag-bin
```

Expected: both checks finish without errors.

- [ ] **Step 3: Run installed MCP smoke**

```powershell
$env:CARGO_BUILD_JOBS='1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\mcp-iterate.ps1 -Build -Install
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\mcp-full-smoke.ps1
```

Expected:

```json
{
  "failures": [],
  "tool_count": 47
}
```

- [ ] **Step 4: Verify Piighost path non-regression**

```powershell
$exe = "$env:LOCALAPPDATA\anno-rag\anno-rag.exe"
$env:ANNO_MODELS_DIR = "$env:USERPROFILE\.anno-rag\models"
& $exe mcp
```

Use the smoke harness or MCP client to call:

```json
{"name":"index","arguments":{"path":"C:\\Users\\NMarchitecte\\Documents\\piighost-test-multi-format","profile":"all"}}
{"name":"status","arguments":{}}
{"name":"search","arguments":{"query":"contrat paiement confidentialite","scope":"legal","allow_cross_corpus":true}}
```

Expected:
- `index.ok == true`
- `status.legal.chunks` is non-null
- legal scoped `search` returns `mode_used == "semantic"` and does not warn that legal was skipped in fast mode

- [ ] **Step 5: Detect changed scope before final handoff**

GitNexus MCP tools are not available in this Codex session. Use git scope instead:

```powershell
git status --short
git diff --stat
```

Expected: only the files listed in this plan changed, plus `Cargo.lock`.

- [ ] **Step 6: Final commit**

If Tasks 1-7 were not committed independently, commit the full implementation:

```powershell
git add docs\superpowers\specs\2026-06-03-mcp-stabilization-fixes-design.md crates\anno-rag-mcp\src crates\anno-rag\src Cargo.toml Cargo.lock crates\anno-rag\Cargo.toml scripts\mcp_full_smoke.py scripts\mcp-full-smoke.ps1
git commit -m "fix: stabilize mcp maintenance and scoped search"
```

Expected: commit succeeds and `git status --short` shows only unrelated pre-existing changes.

---

## Self-Review

- Spec coverage:
  - Forget truthfulness: Task 3.
  - Unified search legal/default all behavior: Task 4.
  - Corpus get/health false positives: Task 5.
  - Status legal count without pipeline init: Task 3.
  - Model inventory file validation and `ANNO_MODELS_DIR`: Task 2.
  - Vault CLI/MCP shared persistence contract: Task 6.
  - Full MCP smoke harness: Task 7.
- Placeholder scan:
  - No steps use open-ended "add handling" language without concrete code or a command.
  - The only conditional is `CorpusStore::list_corpora()` because the implementation must be confirmed against the current store API; the task provides the exact table and columns to query if it is absent.
- Type consistency:
  - `ModelInventoryService` lives in `anno-rag-mcp` and is used only by MCP.
  - `LegalMaintenanceService` uses `anno_rag::store::Store` for chunk count/deletion and `LegalStore`/KG/status only for auxiliary cleanup.
  - `VaultKeyStatus` lives in `anno-rag` so CLI and MCP can share it.
  - Search mode contract is `mode_used` plus `scope_modes`, preserving existing fields while making mixed mode explicit.
