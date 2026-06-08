# Cowork 3P Local-First Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the current Anno MCP server safe for a Cowork 3P local-first rollout by enforcing admin-approved filesystem roots and generating/documenting Cowork-ready MCP configuration.

**Architecture:** Keep Phase 1 local-first: Cowork uses the existing `anno-rag mcp` stdio server for indexing, search, rehydration, and privacy vault workflows. Add a small `AllowedRoots` policy inside `anno-rag-mcp`, wire it into every path-taking MCP entrypoint that Cowork can drive, and update setup/docs so admins can set `ANNO_RAG_ALLOWED_ROOTS`. Do not change provider routing or `/v1/files` in this phase.

**Tech Stack:** Rust 2021, rmcp, Tokio, serde/serde_json, clap, existing `anno-rag-mcp`, `anno-rag-bin setup-mcp`, PowerShell/Bash setup wrappers, `scripts/dev-fast.ps1`, `scripts/test-local.ps1`, GitNexus CLI.

**Spec:** [`docs/superpowers/specs/2026-06-06-cowork-3p-sovereign-gateway-design.md`](../specs/2026-06-06-cowork-3p-sovereign-gateway-design.md)

---

## Code Review Findings

The existing code supports the local privacy workflow but lacks the filesystem guard required for a managed Cowork 3P deployment:

- `crates/anno-rag-mcp/src/lib.rs` exposes `index`, `sync_corpus`, `search`, `rehydrate`, `privacy_prepare_folder`, `privacy_finalize_folder`, and `privacy_status`.
- `privacy_prepare_folder_impl` and `privacy_finalize_folder_impl` pass user/model-supplied paths directly to `Pipeline::privacy_prepare_folder` and `Pipeline::privacy_finalize_folder`.
- `index_impl_routing`, `knowledge_add_local_folder_impl`, `legal_ingest_impl`, `forget_impl_routing`, and `review_export` also accept path strings without an Anno-side root allowlist.
- `crates/anno-rag/src/privacy_workspace.rs` already returns paths/counts only and keeps cleartext inside local working documents.
- `crates/anno-rag-bin/src/setup_mcp.rs` already writes MCP `env` values for `ANNO_MODELS_DIR`, so it is the right place to add `ANNO_RAG_ALLOWED_ROOTS`.
- `anno-privacy-gateway` remains out of scope for this Phase 1 plan. It currently has one upstream Anthropic-compatible base URL, `/v1/files` fail-closed, and streaming `input_json_delta` fail-closed. Those are Phase 2 and Phase 3 concerns.

## Scope Check

The approved spec covers three subsystems. This plan covers only Phase 1, because it is independently shippable and should land before sovereign routing or file-upload interception:

1. Cowork 3P local-first: this plan.
2. Sovereign provider gateway: separate plan after Phase 1 is merged.
3. Gateway file/document ingress: separate plan after provider routing is stable.

Do not edit `crates/anno-privacy-gateway` in this plan.

## File Map

Create:

- `crates/anno-rag-mcp/src/allowed_roots.rs` - parse `ANNO_RAG_ALLOWED_ROOTS`, canonicalize roots, and validate existing input paths or output paths.

Modify:

- `crates/anno-rag-mcp/src/lib.rs` - add `AllowedRoots` to `AnnoRagServer`, validate path-taking tools, and include root enforcement status in `privacy_status`.
- `crates/anno-rag-bin/src/setup_mcp.rs` - add `--allowed-root DIR`, write `ANNO_RAG_ALLOWED_ROOTS` into Desktop JSON and Claude Code args, and include it in manual output.
- `scripts/setup-mcp.ps1` - pass repeated `-AllowedRoot` values through to `anno-rag setup-mcp --allowed-root`.
- `scripts/setup-mcp.sh` - pass repeated `--allowed-root` values through to `anno-rag setup-mcp --allowed-root`.
- `docs/getting-started/claude-desktop-cowork.md` - document `ANNO_RAG_ALLOWED_ROOTS` for Desktop/Cowork and managed 3P rollout.
- `docs/developers/mcp-tools.md` - document root allowlist enforcement and which tools are path-gated.

Do not modify:

- `crates/anno-privacy-gateway/*`
- provider adapter code
- `/v1/files` behavior
- document block behavior
- vault cryptography

## Build And Test Commands

Check for active Rust builds first:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

Targeted checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check -Profile dev-fast
```

Targeted tests:

```powershell
cargo test -p anno-rag-mcp allowed_roots -- --nocapture
cargo test -p anno-rag-mcp rejects_mcp_paths_outside_allowed_roots -- --nocapture
cargo test -p anno-rag-mcp privacy_status_reports_allowed_roots -- --nocapture
cargo test -p anno-rag-mcp index_rejects_outside_path_before_registering_corpus -- --nocapture
cargo test -p anno-rag-bin allowed_roots -- --nocapture
```

Full local package tests only at the final checkpoint:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-bin
```

Avoid `cargo test --workspace`, release builds, all-feature builds, target cleanup, and profile/target changes during this plan.

---

### Task 0: Pre-Flight And Impact Checks

**Files:** none.

- [ ] **Step 1: Confirm branch and dirty state**

Run:

```powershell
git status --short --branch
```

Expected: worktree state is known. If unrelated files are modified, leave them untouched and record them before starting.

- [ ] **Step 2: Confirm no Rust build is already running**

Run:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

Expected: no long-running `cargo` or `rustc`. If a dist/release build is active, wait or stop before targeted checks.

- [ ] **Step 3: Refresh GitNexus if stale**

Run:

```powershell
npx gitnexus status
```

Expected: `Status: ✅ up-to-date`. If stale, run:

```powershell
npx gitnexus analyze
```

Then rerun `npx gitnexus status`.

- [ ] **Step 4: Run impact checks for symbols this plan will modify**

Run:

```powershell
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
npx gitnexus impact --repo anno index_impl_routing --direction upstream
npx gitnexus impact --repo anno privacy_prepare_folder_impl --direction upstream
npx gitnexus impact --repo anno privacy_finalize_folder_impl --direction upstream
npx gitnexus impact --repo anno legal_ingest_impl --direction upstream
npx gitnexus impact --repo anno merge_desktop_config --direction upstream
npx gitnexus impact --repo anno build_claude_code_args --direction upstream
```

Expected: record direct callers and risk. If any result reports HIGH or CRITICAL risk, stop and report the blast radius before editing.

---

### Task 1: Add Allowed Roots Policy

**Files:**
- Create: `crates/anno-rag-mcp/src/allowed_roots.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Test: `crates/anno-rag-mcp/src/allowed_roots.rs`

- [ ] **Step 1: Create failing tests for root parsing and validation**

Create `crates/anno-rag-mcp/src/allowed_roots.rs` with this initial test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn allowed_roots_unset_is_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        assert!(!policy.is_enforced());
        assert_eq!(policy.summary().enforced, false);
        assert_eq!(policy.summary().roots, Vec::<String>::new());
    }

    #[test]
    fn allowed_roots_unset_keeps_relative_paths_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        let validated = policy
            .validate_existing_path("path", "relative-folder")
            .expect("permissive when unset");
        assert_eq!(validated, std::path::PathBuf::from("relative-folder"));
    }

    #[test]
    fn allowed_roots_accepts_path_inside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let client = dir.path().join("client-a");
        let matter = client.join("matter-1");
        std::fs::create_dir_all(&matter).expect("matter dir");

        let raw = client.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let validated = policy
            .validate_existing_path("source_root", &matter)
            .expect("inside root");

        assert!(validated.starts_with(std::fs::canonicalize(&client).expect("canonical root")));
    }

    #[test]
    fn allowed_roots_rejects_path_outside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_path("source_root", &outside)
            .expect_err("outside root rejected");

        assert!(err.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
    }

    #[test]
    fn allowed_roots_rejects_relative_input_paths_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_path("source_root", "relative-folder")
            .expect_err("relative path rejected");

        assert!(err.contains("absolute"));
    }

    #[test]
    fn allowed_roots_validates_output_parent_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let parent = allowed.join("exports");
        std::fs::create_dir_all(&parent).expect("export dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let output = parent.join("review.xlsx");
        let validated = policy
            .validate_output_path("output_path", &output)
            .expect("inside root");

        assert_eq!(validated.file_name().and_then(|s| s.to_str()), Some("review.xlsx"));
    }
}
```

- [ ] **Step 2: Run the module tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag-mcp allowed_roots -- --nocapture
```

Expected: FAIL because `AllowedRoots` does not exist.

- [ ] **Step 3: Implement the root policy**

Replace `crates/anno-rag-mcp/src/allowed_roots.rs` with:

```rust
//! Filesystem allowlist for local MCP path-taking tools.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Environment variable containing semicolon-separated absolute root paths.
pub const ALLOWED_ROOTS_ENV: &str = "ANNO_RAG_ALLOWED_ROOTS";

/// Safe summary returned by status tools.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AllowedRootsSummary {
    /// Whether root enforcement is active.
    pub enforced: bool,
    /// Canonical allowed roots.
    pub roots: Vec<String>,
}

/// Canonical filesystem roots that MCP path inputs may access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedRoots {
    roots: Vec<PathBuf>,
    deny_all: bool,
}

impl AllowedRoots {
    /// Build from `ANNO_RAG_ALLOWED_ROOTS`.
    pub fn from_env() -> Result<Self, String> {
        let raw = std::env::var(ALLOWED_ROOTS_ENV).ok();
        Self::parse(raw.as_deref())
    }

    /// Build a policy that rejects every path. Used when env config is invalid.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            roots: Vec::new(),
            deny_all: true,
        }
    }

    /// Parse semicolon-separated roots. Empty or missing value is permissive.
    pub fn parse(raw: Option<&str>) -> Result<Self, String> {
        let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(Self::default());
        };

        let mut roots = Vec::new();
        for part in raw.split(';').map(str::trim).filter(|part| !part.is_empty()) {
            let path = PathBuf::from(part);
            if !path.is_absolute() {
                return Err(format!("{ALLOWED_ROOTS_ENV} entry must be absolute: {part}"));
            }
            let canonical = std::fs::canonicalize(&path)
                .map_err(|e| format!("canonicalize allowed root {}: {e}", path.display()))?;
            roots.push(canonical);
        }

        if roots.is_empty() {
            return Ok(Self::default());
        }

        roots.sort();
        roots.dedup();
        Ok(Self {
            roots,
            deny_all: false,
        })
    }

    /// Return true when root enforcement is active.
    #[must_use]
    pub fn is_enforced(&self) -> bool {
        self.deny_all || !self.roots.is_empty()
    }

    /// Return a safe status summary with no user-provided input path.
    #[must_use]
    pub fn summary(&self) -> AllowedRootsSummary {
        AllowedRootsSummary {
            enforced: self.is_enforced(),
            roots: self
                .roots
                .iter()
                .map(|root| root.display().to_string())
                .collect(),
        }
    }

    /// Validate an existing input path for a read/index/finalize operation.
    pub fn validate_existing_path(
        &self,
        label: &str,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf, String> {
        let path = path.as_ref();
        if !self.is_enforced() {
            return Ok(path.to_path_buf());
        }

        if !path.is_absolute() {
            return Err(format!("{label} must be an absolute path: {}", path.display()));
        }

        let canonical = std::fs::canonicalize(path)
            .map_err(|e| format!("canonicalize {label} {}: {e}", path.display()))?;

        if !self.contains(&canonical) {
            return Err(format!(
                "{label} is outside {ALLOWED_ROOTS_ENV}: {}",
                canonical.display()
            ));
        }

        Ok(canonical)
    }

    /// Validate a write destination whose parent directory already exists.
    pub fn validate_output_path(
        &self,
        label: &str,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf, String> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(format!("{label} must be an absolute path: {}", path.display()));
        }

        if !self.is_enforced() {
            return Ok(path.to_path_buf());
        }

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| format!("{label} must have a parent directory: {}", path.display()))?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("canonicalize {label} parent {}: {e}", parent.display()))?;
        if !self.contains(&canonical_parent) {
            return Err(format!(
                "{label} is outside {ALLOWED_ROOTS_ENV}: {}",
                path.display()
            ));
        }

        let file_name = path
            .file_name()
            .ok_or_else(|| format!("{label} must include a file name: {}", path.display()))?;
        Ok(canonical_parent.join(file_name))
    }

    fn contains(&self, canonical: &Path) -> bool {
        if self.deny_all {
            return false;
        }

        self.roots
            .iter()
            .any(|root| canonical == root || canonical.starts_with(root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn allowed_roots_unset_is_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        assert!(!policy.is_enforced());
        assert_eq!(policy.summary().enforced, false);
        assert_eq!(policy.summary().roots, Vec::<String>::new());
    }

    #[test]
    fn allowed_roots_unset_keeps_relative_paths_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        let validated = policy
            .validate_existing_path("path", "relative-folder")
            .expect("permissive when unset");
        assert_eq!(validated, std::path::PathBuf::from("relative-folder"));
    }

    #[test]
    fn allowed_roots_accepts_path_inside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let client = dir.path().join("client-a");
        let matter = client.join("matter-1");
        std::fs::create_dir_all(&matter).expect("matter dir");

        let raw = client.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let validated = policy
            .validate_existing_path("source_root", &matter)
            .expect("inside root");

        assert!(validated.starts_with(std::fs::canonicalize(&client).expect("canonical root")));
    }

    #[test]
    fn allowed_roots_rejects_path_outside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_path("source_root", &outside)
            .expect_err("outside root rejected");

        assert!(err.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
    }

    #[test]
    fn allowed_roots_rejects_relative_input_paths_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_path("source_root", "relative-folder")
            .expect_err("relative path rejected");

        assert!(err.contains("absolute"));
    }

    #[test]
    fn allowed_roots_validates_output_parent_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let parent = allowed.join("exports");
        std::fs::create_dir_all(&parent).expect("export dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let output = parent.join("review.xlsx");
        let validated = policy
            .validate_output_path("output_path", &output)
            .expect("inside root");

        assert_eq!(validated.file_name().and_then(|s| s.to_str()), Some("review.xlsx"));
    }
}
```

- [ ] **Step 4: Expose the module**

In `crates/anno-rag-mcp/src/lib.rs`, add the module near the existing module list:

```rust
mod allowed_roots;
```

Add this import near the other `use crate::...` imports:

```rust
use crate::allowed_roots::AllowedRoots;
```

- [ ] **Step 5: Run the module tests and verify they pass**

Run:

```powershell
cargo test -p anno-rag-mcp allowed_roots -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Check the crate**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

---

### Task 2: Wire Allowed Roots Into MCP Path Tools

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add failing server-level tests**

Inside the existing `#[cfg(test)] mod tests` in `crates/anno-rag-mcp/src/lib.rs`, add:

```rust
#[tokio::test]
async fn rejects_mcp_paths_outside_allowed_roots() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let allowed = dir.path().join("allowed");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&allowed).expect("allowed dir");
    std::fs::create_dir_all(&outside).expect("outside dir");

    let policy = crate::allowed_roots::AllowedRoots::parse(Some(
        allowed.to_string_lossy().as_ref(),
    ))
    .expect("allowed roots");
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32])
        .with_allowed_roots_for_test(policy);

    let result = server
        .privacy_prepare_folder_impl(PrivacyPrepareFolderParams {
            source_root: outside.to_string_lossy().to_string(),
            recursive: true,
        })
        .await
        .expect_err("outside path rejected before model load");

    assert!(result.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
}

#[tokio::test]
async fn privacy_status_reports_allowed_roots() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let allowed = dir.path().join("allowed");
    std::fs::create_dir_all(&allowed).expect("allowed dir");

    let policy = crate::allowed_roots::AllowedRoots::parse(Some(
        allowed.to_string_lossy().as_ref(),
    ))
    .expect("allowed roots");
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32])
        .with_allowed_roots_for_test(policy);

    let status = server.privacy_status_impl().await;

    assert_eq!(status["allowed_roots"]["enforced"], true);
    assert_eq!(status["privacy_boundary"], "local");
}

#[tokio::test]
async fn index_rejects_outside_path_before_registering_corpus() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let allowed = dir.path().join("allowed");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&allowed).expect("allowed dir");
    std::fs::create_dir_all(&outside).expect("outside dir");

    let policy = crate::allowed_roots::AllowedRoots::parse(Some(
        allowed.to_string_lossy().as_ref(),
    ))
    .expect("allowed roots");
    let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32])
        .with_allowed_roots_for_test(policy);

    let body = server
        .index_impl_routing(IndexParams {
            path: outside.to_string_lossy().to_string(),
            profile: "general".to_string(),
        })
        .await;
    let value: serde_json::Value = serde_json::from_str(&body).expect("json");

    assert_eq!(value["ok"], false);
    assert!(
        value["error"]
            .as_str()
            .expect("error")
            .contains("outside ANNO_RAG_ALLOWED_ROOTS")
    );
}
```

- [ ] **Step 2: Run the new tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag-mcp rejects_mcp_paths_outside_allowed_roots -- --nocapture
cargo test -p anno-rag-mcp privacy_status_reports_allowed_roots -- --nocapture
cargo test -p anno-rag-mcp index_rejects_outside_path_before_registering_corpus -- --nocapture
```

Expected: FAIL because `with_allowed_roots_for_test`, the server field, and status value do not exist.

- [ ] **Step 3: Add the allowed roots field to `AnnoRagServer`**

In `crates/anno-rag-mcp/src/lib.rs`, add this field to `AnnoRagServer`:

```rust
allowed_roots: AllowedRoots,
```

In both `AnnoRagServer::new` and `AnnoRagServer::new_lazy`, initialize it:

```rust
allowed_roots: AllowedRoots::from_env().unwrap_or_else(|error| {
    tracing::warn!(%error, "invalid ANNO_RAG_ALLOWED_ROOTS; denying MCP path access");
    AllowedRoots::deny_all()
}),
```

- [ ] **Step 4: Add server helpers**

In the `impl AnnoRagServer` block that contains implementation helpers, add:

```rust
fn validate_existing_mcp_path(
    &self,
    label: &str,
    path: impl AsRef<std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    self.allowed_roots.validate_existing_path(label, path)
}

fn validate_output_mcp_path(
    &self,
    label: &str,
    path: impl AsRef<std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    self.allowed_roots.validate_output_path(label, path)
}

#[cfg(test)]
fn with_allowed_roots_for_test(mut self, allowed_roots: AllowedRoots) -> Self {
    self.allowed_roots = allowed_roots;
    self
}
```

- [ ] **Step 5: Include allowed roots in `privacy_status`**

Change `privacy_status_impl` to include:

```rust
"allowed_roots": self.allowed_roots.summary(),
```

The final object should retain the existing safe values:

```rust
serde_json::json!({
    "ok": true,
    "tools": [
        "privacy_prepare_folder",
        "privacy_finalize_folder",
        "privacy_status"
    ],
    "privacy_boundary": "local",
    "returns_document_content": false,
    "allowed_roots": self.allowed_roots.summary()
})
```

- [ ] **Step 6: Gate privacy tools**

In `privacy_prepare_folder_impl`, validate first and pass the canonical path:

```rust
let source_root = self.validate_existing_mcp_path("source_root", &p.source_root)?;
let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
let summary = pipeline
    .privacy_prepare_folder(&source_root, p.recursive)
    .await
    .map_err(|e| e.to_string())?;
```

In `privacy_finalize_folder_impl`, validate first and pass the canonical path:

```rust
let workspace = self.validate_existing_mcp_path("workspace", &p.workspace)?;
let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
let summary = pipeline
    .privacy_finalize_folder(&workspace)
    .await
    .map_err(|e| e.to_string())?;
```

- [ ] **Step 7: Gate indexing and legacy folder registration**

At the top of `index_impl_routing`, after profile validation, add:

```rust
let path = match self.validate_existing_mcp_path("path", &p.path) {
    Ok(path) => path.display().to_string(),
    Err(error) => {
        return serde_json::json!({
            "ok": false,
            "error": error,
        })
        .to_string();
    }
};

let p = IndexParams {
    path,
    profile: p.profile,
};
```

At the top of `knowledge_add_local_folder_impl`, add:

```rust
let path = self
    .validate_existing_mcp_path("path", path)?
    .display()
    .to_string();
```

Then pass `&path` to the knowledge service instead of the raw argument.

- [ ] **Step 8: Gate legal ingest and explicit path forget**

At the top of `legal_ingest_impl`, before `let pipeline = ...`, add:

```rust
let folder = self.validate_existing_mcp_path("folder", &p.folder)?;
let p = LegalIngestParams {
    folder: folder.display().to_string(),
    recursive: p.recursive,
};
```

In `forget_impl_routing`, in the final `else` branch that treats `p.target` as a path, add this before calling `knowledge_forget_by_path` or `forget_folder_path`:

```rust
let target = match self.validate_existing_mcp_path("target", &p.target) {
    Ok(path) => path.display().to_string(),
    Err(error) => {
        return serde_json::json!({
            "ok": false,
            "removed": {
                "knowledge_objects": 0u64,
                "legal_chunks": 0u64,
                "tabular_reviews": 0u64
            },
            "errors": [error],
        })
        .to_string();
    }
};
```

Use `target` for both path forget calls in that branch.

- [ ] **Step 9: Gate XLSX export output paths**

In the `review_export` branch that handles `xlsx`, replace the current `PathBuf::from(&path_str)` absolute-only validation with:

```rust
let path = match self.validate_output_mcp_path("output_path", &path_str) {
    Ok(path) => path,
    Err(error) => return format!("Error: {error}"),
};
```

Keep the existing `anno_rag_tabular::export::export_xlsx(ts, review_id, path.as_path()).await` call.

- [ ] **Step 10: Run the server-level tests**

Run:

```powershell
cargo test -p anno-rag-mcp rejects_mcp_paths_outside_allowed_roots -- --nocapture
cargo test -p anno-rag-mcp privacy_status_reports_allowed_roots -- --nocapture
cargo test -p anno-rag-mcp index_rejects_outside_path_before_registering_corpus -- --nocapture
```

Expected: PASS.

- [ ] **Step 11: Run the targeted MCP package check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 12: Commit the MCP guard**

Run:

```powershell
git add crates\anno-rag-mcp\src\allowed_roots.rs crates\anno-rag-mcp\src\lib.rs
git commit -m "feat: restrict mcp filesystem access roots"
```

Expected: commit succeeds.

---

### Task 3: Add Allowed Roots To Setup MCP Config Generation

**Files:**
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`
- Modify: `scripts/setup-mcp.ps1`
- Modify: `scripts/setup-mcp.sh`
- Test: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Add failing setup tests**

In `crates/anno-rag-bin/src/setup_mcp.rs`, add tests to the existing test module:

```rust
#[test]
fn claude_code_args_include_allowed_roots_env_when_configured() {
    let models_dir = absolute_test_path(&["anno-rag", "models"]);
    let binary = absolute_test_path(&["hacienda", "anno-rag"]);
    let root_a = absolute_test_path(&["clients", "a"]);
    let root_b = absolute_test_path(&["clients", "b"]);

    let args = build_claude_code_args(
        ClaudeCodeScope::User,
        &models_dir,
        &binary,
        &[root_a.clone(), root_b.clone()],
    )
    .expect("args");

    assert!(args.contains(&"--env".to_string()));
    let expected = format!(
        "ANNO_RAG_ALLOWED_ROOTS={};{}",
        path_to_config_string(&root_a).expect("root a"),
        path_to_config_string(&root_b).expect("root b")
    );
    assert!(args.contains(&expected));
}

#[test]
fn desktop_config_includes_allowed_roots_env_when_configured() {
    let binary = absolute_test_path(&["hacienda", "anno-rag.exe"]);
    let models_dir = absolute_test_path(&["anno-rag", "models"]);
    let root = absolute_test_path(&["clients"]);

    let merged =
        merge_desktop_config(json!({}), &binary, &models_dir, true, &[root.clone()])
            .expect("merge");
    let env = merged["mcpServers"]["anno-rag"]["env"]
        .as_object()
        .expect("anno-rag env");

    assert_eq!(
        env.get("ANNO_RAG_ALLOWED_ROOTS").and_then(|v| v.as_str()),
        Some(path_to_config_string(&root).expect("root").as_str())
    );
}
```

- [ ] **Step 2: Run the setup tests and verify they fail**

Run:

```powershell
cargo test -p anno-rag-bin allowed_roots -- --nocapture
```

Expected: FAIL because the function signatures do not accept allowed roots yet.

- [ ] **Step 3: Add CLI args and env formatting**

In `SetupMcpArgs`, add:

```rust
#[arg(long = "allowed-root", value_name = "DIR")]
pub allowed_roots: Vec<PathBuf>,
```

Add helper functions near `path_to_config_string`:

```rust
fn allowed_roots_env_value(allowed_roots: &[PathBuf]) -> anyhow::Result<Option<String>> {
    if allowed_roots.is_empty() {
        return Ok(None);
    }

    let mut roots = Vec::with_capacity(allowed_roots.len());
    for root in allowed_roots {
        validate_absolute_path(root).map_err(|e| anyhow!(e))?;
        roots.push(path_to_config_string(root)?);
    }
    Ok(Some(roots.join(";")))
}
```

- [ ] **Step 4: Update Claude Code args**

Change the signature:

```rust
pub fn build_claude_code_args(
    scope: ClaudeCodeScope,
    models_dir: &Path,
    binary: &Path,
    allowed_roots: &[PathBuf],
) -> anyhow::Result<Vec<String>> {
```

Build `args` mutably:

```rust
let mut args = vec![
    "mcp".to_string(),
    "add".to_string(),
    "--transport".to_string(),
    "stdio".to_string(),
    "--scope".to_string(),
    scope.as_cli_value().to_string(),
    "--env".to_string(),
    format!("ANNO_MODELS_DIR={models_dir}"),
];

if let Some(roots) = allowed_roots_env_value(allowed_roots)? {
    args.push("--env".to_string());
    args.push(format!("ANNO_RAG_ALLOWED_ROOTS={roots}"));
}

args.extend([
    "anno-rag".to_string(),
    "--".to_string(),
    binary,
    "mcp".to_string(),
]);

Ok(args)
```

Update `configure_claude_code` signature to accept `allowed_roots: &[PathBuf]` and pass it through to `build_claude_code_args`.

- [ ] **Step 5: Update Desktop config merge**

Change the signature:

```rust
pub fn merge_desktop_config(
    mut existing: Value,
    binary: &Path,
    models_dir: &Path,
    models_verified: bool,
    allowed_roots: &[PathBuf],
) -> anyhow::Result<Value> {
```

After `ANNO_NO_DOWNLOADS`, add:

```rust
if let Some(roots) = allowed_roots_env_value(allowed_roots)? {
    env.insert("ANNO_RAG_ALLOWED_ROOTS".to_string(), Value::String(roots));
}
```

Update every caller:

```rust
merge_desktop_config(existing, &binary, &models_dir, models_verified, &args.allowed_roots)?
```

For tests that do not configure roots, pass `&[]`. Update every `SetupMcpArgs` test literal with:

```rust
allowed_roots: Vec::new(),
```

- [ ] **Step 6: Update `run` manual output and Claude Code setup**

In `run`, include roots in the summary:

```rust
if let Some(roots) = allowed_roots_env_value(&args.allowed_roots)? {
    summary.push(format!("allowed_roots: {roots}"));
} else {
    summary.push("allowed_roots: not configured".to_string());
}
```

Pass `&args.allowed_roots` to `build_claude_code_args` and `configure_claude_code`.

- [ ] **Step 7: Update PowerShell wrapper**

In `scripts/setup-mcp.ps1`, add this parameter:

```powershell
[string[]]$AllowedRoot = @()
```

After `$setupArgs` is initialized, add:

```powershell
foreach ($root in $AllowedRoot) {
    $setupArgs += @("--allowed-root", $root)
}
```

Use it like:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target all -AllowedRoot C:\Clients -AllowedRoot D:\Dossiers
```

- [ ] **Step 8: Update Bash wrapper**

In `scripts/setup-mcp.sh`, initialize:

```bash
allowed_roots=()
```

Update the usage text to include:

```bash
[--allowed-root DIR]
```

In argument parsing, add:

```bash
--allowed-root)
  allowed_roots+=("$2")
  shift 2
  ;;
```

Before invoking the binary, add:

```bash
for root in "${allowed_roots[@]}"; do
  args+=(--allowed-root "${root}")
done
```

- [ ] **Step 9: Run setup tests**

Run:

```powershell
cargo test -p anno-rag-bin allowed_roots -- --nocapture
```

Expected: PASS.

- [ ] **Step 10: Run targeted bin check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check -Profile dev-fast
```

Expected: PASS.

- [ ] **Step 11: Commit setup changes**

Run:

```powershell
git add crates\anno-rag-bin\src\setup_mcp.rs scripts\setup-mcp.ps1 scripts\setup-mcp.sh
git commit -m "feat: configure mcp allowed roots"
```

Expected: commit succeeds.

---

### Task 4: Document Cowork 3P Local-First Setup

**Files:**
- Modify: `docs/getting-started/claude-desktop-cowork.md`
- Modify: `docs/developers/mcp-tools.md`

- [ ] **Step 1: Update Desktop/Cowork setup docs**

In `docs/getting-started/claude-desktop-cowork.md`, add a section after the existing Desktop config examples:

````markdown
## Filesystem Root Allowlist

For Cowork or managed Desktop deployments, set `ANNO_RAG_ALLOWED_ROOTS` in the
MCP server environment. The value is a semicolon-separated list of absolute
directories that Anno tools may read from or write into.

Windows example:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\Users\\you\\Tools\\hacienda-v0.11.0-rc.11\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "C:\\Users\\you\\.anno-rag\\models",
        "ANNO_RAG_ALLOWED_ROOTS": "C:\\Clients;D:\\Dossiers"
      }
    }
  }
}
```

macOS example:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/Users/you/Tools/hacienda-v0.11.0-rc.11/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "/Users/you/.anno-rag/models",
        "ANNO_RAG_ALLOWED_ROOTS": "/Users/you/Clients;/Volumes/Dossiers"
      }
    }
  }
}
```

When this variable is set, Anno rejects path-taking MCP calls outside the
configured roots before loading models or touching the filesystem target. When
it is not set, Anno keeps the current permissive local behavior for existing
single-user installs.
````

Also update the setup command examples:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target all -AllowedRoot C:\Clients -AllowedRoot D:\Dossiers
```

```bash
./scripts/setup-mcp.sh --target all --allowed-root "$HOME/Clients" --allowed-root "/Volumes/Dossiers"
```

- [ ] **Step 2: Add managed Cowork 3P note**

In the same doc, add:

````markdown
## Cowork 3P Managed MCP Notes

For Cowork on 3P, use the organization's managed configuration to launch the
same local stdio server:

```json
{
  "name": "anno-rag",
  "transport": "stdio",
  "command": "C:\\Program Files\\Hacienda\\anno-rag.exe",
  "args": ["mcp"],
  "env": {
    "ANNO_MODELS_DIR": "C:\\ProgramData\\Hacienda\\models",
    "ANNO_RAG_ALLOWED_ROOTS": "C:\\Clients;D:\\Dossiers"
  }
}
```

Cowork workspace folder restrictions are still useful, but Anno enforces the
same roots inside the MCP server because tool arguments are model-controlled
inputs.
````

- [ ] **Step 3: Update MCP tools developer docs**

In `docs/developers/mcp-tools.md`, add a short section after the Core Tools table:

````markdown
## Filesystem Boundary

`ANNO_RAG_ALLOWED_ROOTS` constrains MCP tools that accept local filesystem
paths. The value is a semicolon-separated list of absolute directories. When it
is set, these tools reject paths outside the configured roots before loading
models or touching the requested target:

- `index`
- `knowledge_add_local_folder`
- `legal_ingest`
- `privacy_prepare_folder`
- `privacy_finalize_folder`
- explicit path mode in `forget`
- `review_export` when writing `xlsx` to `output_path`

`search`, `sync_corpus`, `sources`, `status`, `privacy_status`, and
`vault_stats` do not accept raw filesystem paths. They operate on already
registered corpus/source identifiers or local aggregate state.
````

- [ ] **Step 4: Run docs checks available in the repo**

Run:

```powershell
rg -n "ANNO_RAG_ALLOWED_ROOTS|allowed-root|AllowedRoot" docs crates scripts
```

Expected: the new docs, setup args, and MCP policy appear.

- [ ] **Step 5: Commit docs**

Run:

```powershell
git add docs\getting-started\claude-desktop-cowork.md docs\developers\mcp-tools.md
git commit -m "docs: document cowork mcp allowed roots"
```

Expected: commit succeeds.

---

### Task 5: Final Verification And Handoff

**Files:** all files changed by Tasks 1-4.

- [ ] **Step 1: Confirm no broad build is active**

Run:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

Expected: no unrelated long-running Rust build.

- [ ] **Step 2: Run targeted package tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-bin
```

Expected: PASS.

- [ ] **Step 3: Run dry-run setup smoke**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target manual -Source local-build -DryRun -AllowedRoot C:\Clients -AllowedRoot D:\Dossiers
```

Expected: output contains `ANNO_RAG_ALLOWED_ROOTS=C:\Clients;D:\Dossiers` and does not write Desktop config.

- [ ] **Step 4: Run diff and secret checks**

Run:

```powershell
git diff --check
rg -n "MISTRAL_API_KEY|SCALEWAY_API_KEY|OVH_AI_ENDPOINTS_ACCESS_TOKEN|ANNO_RAG_VAULT_PASSPHRASE\\s*=" crates scripts docs
```

Expected: `git diff --check` prints no errors. The `rg` command may find docs or env variable names, but it must not find real secret values.

- [ ] **Step 5: Refresh GitNexus after commits**

Run:

```powershell
npx gitnexus analyze
npx gitnexus status
```

Expected: status is up to date on the final commit.

- [ ] **Step 6: Review final commit history**

Run:

```powershell
git log --oneline -3
git status --short
```

Expected: three small commits from this plan and a clean worktree. If GitNexus rewrites only generated context counters in `AGENTS.md` or `CLAUDE.md`, include them in the final docs commit or amend the relevant commit so the worktree is clean.

---

## Acceptance Criteria

- `ANNO_RAG_ALLOWED_ROOTS` exists and is parsed as semicolon-separated absolute roots.
- With `ANNO_RAG_ALLOWED_ROOTS` unset, existing local single-user behavior stays permissive.
- With `ANNO_RAG_ALLOWED_ROOTS` set, path-taking MCP tools reject outside paths before model load or filesystem work.
- `privacy_status` reports whether root enforcement is active without returning document content.
- `setup-mcp` can write `ANNO_RAG_ALLOWED_ROOTS` into Claude Desktop JSON and Claude Code CLI args.
- PowerShell and Bash setup wrappers pass allowed roots through.
- Cowork/Desktop setup docs show local and managed 3P examples.
- Targeted `anno-rag-mcp` and `anno-rag-bin` checks/tests pass.

## Phase 2 Handoff

After this plan is implemented and reviewed, create a separate plan for sovereign provider routing in `anno-privacy-gateway`:

- model catalog with provider plus privacy-mode IDs;
- provider config profiles for Mistral, Scaleway, OVHcloud, and local providers;
- DPA-gated `cleartext_dpa`;
- OpenAI-compatible provider adapter;
- Cowork-compatible streaming tool-use support.

Do not start Phase 2 until Phase 1 has a clean test run and reviewed diff.
