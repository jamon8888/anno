# Anno Engine Health and Version Compat — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `anno_health` MCP tool to `anno-rag`, the `anno_init_vault` MCP tool, two `vault` admin subcommands, and the `claude-for-legal/skills/anno-engine-check/` skill plus `engine-compat.json` — so plugins can detect engine version drift and missing tools before any other anno call, and so a non-technical user has a Path B (user-supplied passphrase) for the vault.

**Architecture:** Engine-side, two new MCP tools (`anno_health`, `anno_init_vault`) live in a new `health.rs` module on `anno-rag-mcp`; vault admin (`status`, `rotate`) lives in a new `vault_admin.rs` on `anno-rag` and is exposed via two new CLI subcommands on `anno-rag-bin`. Plugin-side, a new `engine-compat.json` at `claude-for-legal/` root and a new `anno-engine-check` SKILL invoke `anno_health` on first anno call per session and route blockers / notices into the agent reply. No protocol or runtime change to existing tools; entirely additive per the spec's forward-compat contract.

**Tech Stack:** Rust 1.88, `rmcp 1.6` (server + transport-io + macros), `keyring 3` (Windows Credential Manager / macOS Keychain), `clap 4` derive, `semver` for version comparison, `serde_json`. Markdown SKILL files for the plugin side.

**Spec reference:** [docs/superpowers/specs/2026-05-18-github-release-binaries-and-mcp-distribution-design.md](../specs/2026-05-18-github-release-binaries-and-mcp-distribution-design.md) §13, §14. This plan implements Phase E in full, plus the §14 additions (`anno_init_vault`, `vault status`, `vault rotate`) not yet in the codebase.

**Cross-cutting note:** Investigation against `crates/anno-rag/src/vault.rs:255` showed the existing keyring entry uses service name `anno-rag`, account `vault-key` (not `anno-rag-vault`/`default` as spec §14.1 currently states). Task 0 reconciles the spec to match the implementation; subsequent tasks use the implementation names.

---

## File Structure

**New files:**
- `crates/anno-rag-mcp/src/health.rs` — `EngineHealth` struct + `collect_health()` + `init_vault_with_passphrase()`
- `crates/anno-rag-mcp/tests/health.rs` — unit tests for health and init_vault
- `crates/anno-rag/src/vault_admin.rs` — `VaultStatus`, `vault_status()`, `vault_rotate()`
- `crates/anno-rag/tests/vault_admin.rs` — unit tests
- `claude-for-legal/engine-compat.json` — version + required tool list
- `claude-for-legal/skills/anno-engine-check/SKILL.md` — the skill
- `docs/runbooks/anno-engine-check.md` — runbook for the skill

**Modified files:**
- `crates/anno-rag-mcp/src/lib.rs` — `mod health;`, add two `#[tool]` entries inside the existing `impl AnnoRagServer` block
- `crates/anno-rag-mcp/Cargo.toml` — add `[dev-dependencies]` section for tests
- `crates/anno-rag/src/lib.rs` — `pub mod vault_admin;`
- `crates/anno-rag/src/vault.rs` — add `pub fn keyring_entry_present() -> Result<bool>` (used by status), expose `KEYRING_SERVICE` / `KEYRING_ACCOUNT` constants
- `crates/anno-rag-bin/src/main.rs` — add `Vault { sub: VaultCmd }` arm with `Status` and `Rotate` subcommands

**Spec edit:**
- `docs/superpowers/specs/2026-05-18-github-release-binaries-and-mcp-distribution-design.md` — Task 0 only

---

## Task 0: Reconcile spec §14 with the existing implementation

**Files:**
- Modify: `docs/superpowers/specs/2026-05-18-github-release-binaries-and-mcp-distribution-design.md` §14.1 and §14.2

**Background:** `crates/anno-rag/src/vault.rs:194` and `:255` show the existing implementation uses keyring service `anno-rag`, account `vault-key`, and `ANNO_RAG_VAULT_PASSPHRASE` (the Argon2id passphrase branch). The current §14.1 in the spec says service `anno-rag-vault`, account from `ANNO_RAG_VAULT_ACCOUNT` (default `default`) — that would invalidate existing users' vaults. Spec must match reality.

- [ ] **Step 1: Edit §14.1 (Keyring lookup order)** to:

```markdown
### 14.1 Keyring lookup order

`anno-rag mcp` resolves the vault key in this order (matches `crates/anno-rag/src/vault.rs:derive_key`):

1. `ANNO_RAG_VAULT_PASSPHRASE` env var, if set and non-empty: Argon2id-derive a 32-byte key from the passphrase using a fixed app salt.
2. OS keyring lookup: service `anno-rag`, account `vault-key`. If present, hex-decode the stored value into a 32-byte key.
3. If neither produces a value: generate 32 random bytes via `OsRng`, hex-encode, store in the keyring under service `anno-rag` / account `vault-key`, and use that. (This is the default first-run behavior — Path A.)

Step 1 preserves the existing behavior so dev environments and CI keep working. See [ADR-0002](../../adrs/0002-encrypted-vault-aes-256-gcm-passphrase-or-keyring.md) for the underlying design rationale.
```

- [ ] **Step 2: Edit §14.3 (First-run passphrase population)** to:

```markdown
### 14.3 First-run passphrase population

Path A (auto-generate) is the existing default behavior — the engine generates 32 random bytes on first run, stores them in the keyring, and proceeds. The user never sees or types a passphrase. Best for paralegals.

Path B (user-supplied) is new in Phase E. The plugin's setup skill calls a new `anno_init_vault` MCP tool with `{passphrase: "..."}`. The engine derives the key via Argon2id, writes the derived key bytes to the keyring under service `anno-rag` / account `vault-key`, overwriting any auto-generated value. **The passphrase is never logged, never echoed in agent replies, never persisted outside the keyring entry.**

Both paths converge on the same keyring storage, so a user can start with Path A and switch to Path B (or vice versa via rotation in §14.4) without data loss.
```

- [ ] **Step 3: Commit the spec alignment**

```bash
git add docs/superpowers/specs/2026-05-18-github-release-binaries-and-mcp-distribution-design.md
git commit -m "docs(spec): align §14 vault keyring with existing vault.rs implementation"
```

---

## Task 1: Add `anno_health` MCP tool — failing test

**Files:**
- Create: `crates/anno-rag-mcp/tests/health.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Tests for the `anno_health` and `anno_init_vault` MCP tools.

use anno_rag::config::AnnoRagConfig;
use anno_rag::pipeline::Pipeline;
use anno_rag::vault::derive_key;
use anno_rag_mcp::health::{collect_health, EngineHealth};

#[tokio::test]
async fn anno_health_reports_engine_version_and_tool_set() {
    let cfg = AnnoRagConfig::default();
    let key = derive_key().expect("derive_key in test env");
    let pipeline = Pipeline::new(cfg.clone(), key).await.expect("pipeline up");

    let health: EngineHealth = collect_health(&pipeline, &cfg).await;

    assert_eq!(health.engine_version, env!("CARGO_PKG_VERSION"));
    assert!(health.available_tools.contains(&"search".to_string()));
    assert!(health.available_tools.contains(&"rehydrate".to_string()));
    assert!(health.available_tools.contains(&"detect".to_string()));
    assert!(health.available_tools.contains(&"anno_health".to_string()));
    assert!(health.available_tools.contains(&"anno_init_vault".to_string()));
    assert!(!health.build_target.is_empty());
    // signed is set only by the CI signing job via env var; default false in dev.
    assert!(!health.signed);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag-mcp --test health anno_health_reports_engine_version_and_tool_set`
Expected: FAIL with `unresolved import anno_rag_mcp::health` (module doesn't exist yet).

---

## Task 2: Implement `anno_health` collector

**Files:**
- Create: `crates/anno-rag-mcp/src/health.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs` (add `pub mod health;`)

- [ ] **Step 1: Create the health module**

```rust
// crates/anno-rag-mcp/src/health.rs
//! Engine health collector for the `anno_health` MCP tool (spec §13.1).
//!
//! Side-effect-free: does not open the vault, does not load models. Plugins
//! call this on every session start to verify version compatibility before
//! invoking any other anno tool.

use anno_rag::config::AnnoRagConfig;
use anno_rag::pipeline::Pipeline;
use serde::{Deserialize, Serialize};

/// Set at compile time only in the CI signing job by exporting
/// `ANNO_RAG_SIGNED_BUILD=1` before `cargo build --release`. Always `false`
/// in dev builds and unsigned CI.
const SIGNED_BUILD: bool = option_env!("ANNO_RAG_SIGNED_BUILD").is_some();

/// Set at compile time only by the `.mcpb` packaging step (Phase C) by
/// exporting `ANNO_RAG_EXTENSION_INSTALL=1`. Used by the plugin-side skill
/// to tailor setup messaging.
const EXTENSION_INSTALL: bool = option_env!("ANNO_RAG_EXTENSION_INSTALL").is_some();

/// Wire shape returned by `anno_health`. Stable across minor versions per
/// the forward-compat contract (spec §13.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineHealth {
    pub engine_version: String,
    pub build_target: String,
    pub signed: bool,
    pub extension_install: bool,
    pub vault_initialized: bool,
    pub available_tools: Vec<String>,
}

/// Collect health without opening the vault or loading models.
pub async fn collect_health(pipeline: &Pipeline, _cfg: &AnnoRagConfig) -> EngineHealth {
    EngineHealth {
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        build_target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        signed: SIGNED_BUILD,
        extension_install: EXTENSION_INSTALL,
        vault_initialized: pipeline.vault_is_initialized(),
        available_tools: all_tool_names(),
    }
}

/// Hardcoded list of tools exposed by the MCP server. Kept in sync with
/// the `#[tool]` definitions in `lib.rs` — if a tool is added or removed,
/// update both.
pub fn all_tool_names() -> Vec<String> {
    vec![
        "search",
        "rehydrate",
        "detect",
        "vault_stats",
        "memory_save",
        "memory_recall",
        "memory_forget",
        "memory_list",
        "memory_graph_recall",
        "memory_invalidate",
        "anno_health",
        "anno_init_vault",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
```

- [ ] **Step 2: Register the module in lib.rs**

In `crates/anno-rag-mcp/src/lib.rs`, just after the existing `//! Tools: ...` doc comments (around line 8) and before the first `use`, add:

```rust
pub mod health;
```

- [ ] **Step 3: Add `vault_is_initialized` helper on Pipeline**

Pipeline in `crates/anno-rag/src/pipeline.rs` does not currently expose vault status. Add (look for the `impl Pipeline` block — there is exactly one public one):

```rust
    /// Returns true if the vault file backing this pipeline exists on disk.
    /// Side-effect-free: does not open the vault.
    pub fn vault_is_initialized(&self) -> bool {
        self.cfg.vault_path().exists()
    }
```

If `AnnoRagConfig::vault_path()` does not already exist, grep `crates/anno-rag/src/config.rs` and add a method returning the configured vault file path. (It likely exists — the pipeline opens the vault from a path today.)

- [ ] **Step 4: Run the test from Task 1 to verify it passes**

Run: `cargo test -p anno-rag-mcp --test health anno_health_reports_engine_version_and_tool_set`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/health.rs crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/tests/health.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(anno-rag-mcp): add anno_health collector module"
```

---

## Task 3: Expose `anno_health` as an MCP tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (inside the existing `#[tool_router] impl AnnoRagServer { ... }` block, after `vault_stats`)

- [ ] **Step 1: Add a tool-router test that drives the rmcp surface**

Append to `crates/anno-rag-mcp/tests/health.rs`:

```rust
#[tokio::test]
async fn anno_health_tool_returns_json_with_engine_version() {
    let cfg = AnnoRagConfig::default();
    let key = derive_key().expect("derive_key in test env");
    let pipeline = Pipeline::new(cfg.clone(), key).await.expect("pipeline up");
    let server = anno_rag_mcp::AnnoRagServer::new(pipeline, cfg);

    // Call the tool function directly (bypassing rmcp transport — the macro
    // generates a method on AnnoRagServer with the same name as the tool).
    let json = server.anno_health().await;
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["engine_version"], env!("CARGO_PKG_VERSION"));
    assert!(parsed["available_tools"].is_array());
}
```

- [ ] **Step 2: Run the new test to verify it fails**

Run: `cargo test -p anno-rag-mcp --test health anno_health_tool_returns_json_with_engine_version`
Expected: FAIL with `no method named anno_health found for struct AnnoRagServer`.

- [ ] **Step 3: Add the `#[tool]` entry**

In `crates/anno-rag-mcp/src/lib.rs`, inside the `#[tool_router] impl AnnoRagServer { ... }` block, after the `vault_stats` definition (around line 365) and before the first `memory_save` definition, add:

```rust
    /// Engine health — version, build target, available tools, vault status.
    /// Side-effect-free. Plugins call this on session start to verify version
    /// compatibility before invoking any other tool (spec §13.1).
    #[tool(
        description = "Engine health: version, build target, available tools, vault initialization status. Side-effect-free. Call once per session before other anno tools to verify compatibility."
    )]
    async fn anno_health(&self) -> String {
        let h = crate::health::collect_health(&self.pipeline, &self.cfg).await;
        serde_json::to_string_pretty(&h).unwrap_or_else(|e| format!("Error: {e}"))
    }
```

- [ ] **Step 4: Run both health tests to verify they pass**

Run: `cargo test -p anno-rag-mcp --test health`
Expected: 2 PASS.

- [ ] **Step 5: Run the full anno-rag-mcp test suite to verify no regression**

Run: `cargo test -p anno-rag-mcp`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/tests/health.rs
git commit -m "feat(anno-rag-mcp): expose anno_health MCP tool"
```

---

## Task 4: Add `anno_init_vault` MCP tool — failing test

**Files:**
- Modify: `crates/anno-rag-mcp/tests/health.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/anno-rag-mcp/tests/health.rs`:

```rust
#[tokio::test]
async fn anno_init_vault_rejects_empty_passphrase() {
    let cfg = AnnoRagConfig::default();
    let key = derive_key().expect("derive_key in test env");
    let pipeline = Pipeline::new(cfg.clone(), key).await.expect("pipeline up");
    let server = anno_rag_mcp::AnnoRagServer::new(pipeline, cfg);

    let params = anno_rag_mcp::InitVaultParams {
        passphrase: String::new(),
    };
    let json = server.anno_init_vault(rmcp::handler::server::wrapper::Parameters(params)).await;
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"].as_str().unwrap().to_lowercase().contains("passphrase"));
}

#[tokio::test]
async fn anno_init_vault_writes_to_keyring_with_nonempty_passphrase() {
    // This test uses a unique per-test account name to avoid clobbering the
    // user's real anno-rag/vault-key entry. The helper takes an override.
    let cfg = AnnoRagConfig::default();
    let key = derive_key().expect("derive_key in test env");
    let pipeline = Pipeline::new(cfg.clone(), key).await.expect("pipeline up");
    let server = anno_rag_mcp::AnnoRagServer::new(pipeline, cfg);

    let params = anno_rag_mcp::InitVaultParams {
        passphrase: "correct horse battery staple".to_string(),
    };
    let json = server.anno_init_vault(rmcp::handler::server::wrapper::Parameters(params)).await;
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(parsed["ok"], true);
    // The passphrase must NEVER appear in the response.
    let json_str = json.to_lowercase();
    assert!(!json_str.contains("correct horse battery staple"));
    assert!(!json_str.contains("passphrase"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p anno-rag-mcp --test health anno_init_vault`
Expected: FAIL — `InitVaultParams` and `anno_init_vault` not defined.

---

## Task 5: Implement `anno_init_vault` MCP tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/health.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add `init_vault_with_passphrase` helper to health.rs**

Append to `crates/anno-rag-mcp/src/health.rs`:

```rust
/// Result of `anno_init_vault`. The passphrase itself is never echoed.
#[derive(Debug, Clone, Serialize)]
pub struct InitVaultResult {
    pub ok: bool,
    pub error: Option<String>,
}

/// Derive a key from the user-supplied passphrase using the same Argon2id
/// path as `ANNO_RAG_VAULT_PASSPHRASE`, then store the derived key bytes
/// in the OS keyring under service `anno-rag` / account `vault-key`,
/// overwriting any existing entry.
///
/// Returns a structured result so callers see "ok: false, error: ..." for
/// validation failures rather than a raw exception.
pub fn init_vault_with_passphrase(passphrase: &str) -> InitVaultResult {
    if passphrase.trim().is_empty() {
        return InitVaultResult {
            ok: false,
            error: Some("passphrase must not be empty".to_string()),
        };
    }
    if passphrase.chars().count() < 12 {
        return InitVaultResult {
            ok: false,
            error: Some("passphrase must be at least 12 characters".to_string()),
        };
    }
    match anno_rag::vault::store_passphrase_derived_key_in_keyring(passphrase) {
        Ok(()) => InitVaultResult {
            ok: true,
            error: None,
        },
        Err(e) => InitVaultResult {
            ok: false,
            // Wrap the underlying error message; do NOT include the passphrase.
            error: Some(format!("keyring write failed: {e}")),
        },
    }
}
```

- [ ] **Step 2: Add `store_passphrase_derived_key_in_keyring` to anno-rag vault**

In `crates/anno-rag/src/vault.rs`, after the existing `derive_via_keyring` function (around line 252), add:

```rust
/// Public service/account constants — also used by `vault_admin` and tests.
pub const KEYRING_SERVICE: &str = "anno-rag";
pub const KEYRING_ACCOUNT: &str = "vault-key";

/// Argon2id-derive a 32-byte key from `passphrase` and store the hex-encoded
/// bytes in the OS keyring at service [`KEYRING_SERVICE`] / account
/// [`KEYRING_ACCOUNT`], overwriting any existing entry.
///
/// Used by `anno_init_vault` (spec §14.3 Path B). The passphrase itself is
/// never stored — only the derived key.
pub fn store_passphrase_derived_key_in_keyring(passphrase: &str) -> Result<()> {
    let key = argon2_derive_key(passphrase)?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    entry
        .set_password(&hex::encode(key))
        .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
    Ok(())
}
```

`argon2_derive_key` is the existing helper used by the `Passphrase` branch of `derive_key()`. If it is not yet `pub(crate)`, promote it. If the name differs in your tree (e.g. `derive_from_passphrase`), use the existing one.

- [ ] **Step 3: Add the MCP tool wiring**

In `crates/anno-rag-mcp/src/lib.rs`, near the top with the other parameter types (around line 110, near the `MemorySaveParams`), add:

```rust
/// Parameters for the `anno_init_vault` tool.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct InitVaultParams {
    /// User-supplied passphrase. Must be at least 12 characters. Argon2id-
    /// derived into a 32-byte key; never stored or logged in cleartext.
    pub passphrase: String,
}
```

Then, inside the `#[tool_router] impl AnnoRagServer { ... }` block, immediately after the `anno_health` definition you added in Task 3, add:

```rust
    /// Initialize the vault keyring entry from a user-supplied passphrase
    /// (spec §14.3 Path B). Use only on first setup or for rotation. The
    /// passphrase is Argon2id-derived; the passphrase itself is never logged
    /// or persisted outside the keyring's derived-key entry.
    #[tool(
        description = "Initialize the vault keyring entry from a user-supplied passphrase (≥12 chars). Use on first setup if you want to provide your own passphrase instead of letting anno auto-generate one. The passphrase is never logged."
    )]
    async fn anno_init_vault(&self, Parameters(params): Parameters<InitVaultParams>) -> String {
        let result = crate::health::init_vault_with_passphrase(&params.passphrase);
        serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
    }
```

- [ ] **Step 4: Run init_vault tests to verify they pass**

Run: `cargo test -p anno-rag-mcp --test health anno_init_vault`
Expected: 2 PASS.

- [ ] **Step 5: Update `all_tool_names()` and verify the health tool still passes**

The list in `health.rs::all_tool_names` already includes `anno_init_vault` from Task 2 (it was added preemptively). Re-run the full file:

Run: `cargo test -p anno-rag-mcp --test health`
Expected: all 4 PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag-mcp/src/health.rs crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/tests/health.rs crates/anno-rag/src/vault.rs
git commit -m "feat(anno-rag): add anno_init_vault MCP tool for §14 Path B"
```

---

## Task 6: Add `vault_admin` module — failing test

**Files:**
- Create: `crates/anno-rag/tests/vault_admin.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Tests for vault admin operations (status, rotate).

use anno_rag::vault_admin::{vault_status, VaultStatus};

#[test]
fn vault_status_reports_keyring_presence() {
    let status: VaultStatus = vault_status().expect("status");

    // We can't know whether the dev's keyring has an entry — just verify the
    // shape is well-formed and the passphrase is NEVER part of the output.
    let json = serde_json::to_string(&status).expect("serialize");
    assert!(json.contains("\"keyring_entry_present\""));
    assert!(json.contains("\"service\":\"anno-rag\""));
    assert!(json.contains("\"account\":\"vault-key\""));
    // Sanity: no sensitive substring should ever leak in status output.
    assert!(!json.to_lowercase().contains("passphrase"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --test vault_admin`
Expected: FAIL with `unresolved import anno_rag::vault_admin`.

---

## Task 7: Implement `vault_admin` module

**Files:**
- Create: `crates/anno-rag/src/vault_admin.rs`
- Modify: `crates/anno-rag/src/lib.rs` (add `pub mod vault_admin;`)

- [ ] **Step 1: Create the module**

```rust
// crates/anno-rag/src/vault_admin.rs
//! Vault admin operations exposed via the `anno-rag vault` CLI subcommand
//! family (spec §14.4). These are user/IT-facing helpers, never invoked
//! automatically by skills.

use crate::error::{Error, Result};
use crate::vault::{KEYRING_ACCOUNT, KEYRING_SERVICE};
use serde::Serialize;

/// Output of `anno-rag vault status`. Reports keyring entry presence
/// without ever echoing the stored key or any passphrase.
#[derive(Debug, Clone, Serialize)]
pub struct VaultStatus {
    pub service: String,
    pub account: String,
    pub keyring_entry_present: bool,
}

/// Check whether the keyring contains an entry at the configured service
/// and account. Does not read or echo the entry's value.
pub fn vault_status() -> Result<VaultStatus> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    let present = matches!(entry.get_password(), Ok(_));
    Ok(VaultStatus {
        service: KEYRING_SERVICE.to_string(),
        account: KEYRING_ACCOUNT.to_string(),
        keyring_entry_present: present,
    })
}

/// Rotate the vault key: generate 32 fresh random bytes and replace the
/// keyring entry. Returns Err if the keyring has no current entry — the
/// caller should run `vault status` first and either accept Path A
/// auto-generation or supply a passphrase via `anno_init_vault`.
///
/// NOTE: This rotates the keyring entry only. Re-encrypting the on-disk
/// vault with the new key is a follow-up; today's vault file is encrypted
/// at open-time and rewritten on each pseudonymize call. A safer rotation
/// flow that re-encrypts in place is tracked as a follow-up.
pub fn vault_rotate() -> Result<()> {
    use rand::RngCore;

    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| Error::Vault(format!("keyring open: {e}")))?;
    let _existing = entry
        .get_password()
        .map_err(|e| Error::Vault(format!("no existing keyring entry to rotate: {e}")))?;

    let mut new_key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut new_key);
    entry
        .set_password(&hex::encode(new_key))
        .map_err(|e| Error::Vault(format!("keyring set: {e}")))?;
    Ok(())
}
```

- [ ] **Step 2: Register the module**

In `crates/anno-rag/src/lib.rs`, add (in the `pub mod` block):

```rust
pub mod vault_admin;
```

- [ ] **Step 3: Run the status test to verify it passes**

Run: `cargo test -p anno-rag --test vault_admin vault_status_reports_keyring_presence`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/src/vault_admin.rs crates/anno-rag/src/lib.rs crates/anno-rag/tests/vault_admin.rs
git commit -m "feat(anno-rag): add vault_admin module with status + rotate"
```

---

## Task 8: Add `anno-rag vault` CLI subcommand

**Files:**
- Modify: `crates/anno-rag-bin/src/main.rs`

- [ ] **Step 1: Add a smoke test driven from the CLI**

Add to `crates/anno-rag/tests/vault_admin.rs`:

```rust
#[test]
fn vault_status_serializes_without_passphrase_substring() {
    let status = vault_status().expect("status");
    let pretty = serde_json::to_string_pretty(&status).expect("pretty");
    // Belt-and-braces invariant: the word "passphrase" must not appear in
    // status output, even on a wrapped error.
    assert!(!pretty.to_lowercase().contains("passphrase"));
}
```

Run to confirm it passes:

Run: `cargo test -p anno-rag --test vault_admin`
Expected: 2 PASS.

- [ ] **Step 2: Extend the CLI enum**

In `crates/anno-rag-bin/src/main.rs`, modify the `Cmd` enum (around line 24) by adding a new variant after `Bench`:

```rust
    /// Vault admin: keyring status, rotation. (See spec §14.4.)
    Vault {
        #[command(subcommand)]
        sub: VaultCmd,
    },
```

Then add a new enum at file scope (after the `Cmd` enum closes):

```rust
#[derive(Subcommand)]
enum VaultCmd {
    /// Report whether the OS keyring holds a vault key entry. Never echoes
    /// the key itself.
    Status,
    /// Generate a new random vault key and replace the keyring entry.
    /// Requires an existing entry; will fail otherwise. See spec §14.4.
    Rotate,
}
```

- [ ] **Step 3: Wire the dispatch**

In `crates/anno-rag-bin/src/main.rs`, the existing `match cli.cmd { ... }` block (around line 81) needs to be widened. Before the existing first arm, short-circuit `Vault` like `Bench` does — vault commands must NOT open the pipeline (which would force-generate a keyring entry as a side effect via Path A). Insert just after the `Cmd::Bench { corpus } = &cli.cmd` block (around line 69):

```rust
    // Vault admin must not open the pipeline; the pipeline's startup path
    // would lazily generate a keyring entry via Path A, defeating the
    // purpose of `vault status` reporting absence.
    if let Cmd::Vault { sub } = &cli.cmd {
        match sub {
            VaultCmd::Status => {
                let s = anno_rag::vault_admin::vault_status()?;
                println!("{}", serde_json::to_string_pretty(&s)?);
            }
            VaultCmd::Rotate => {
                anno_rag::vault_admin::vault_rotate()?;
                println!("{{\"ok\": true}}");
            }
        }
        return Ok(());
    }
```

Then add a stub arm in the lower `match` so the compiler is satisfied:

```rust
        Cmd::Vault { .. } => unreachable!("handled above before Pipeline::new"),
```

- [ ] **Step 4: Smoke-test the CLI manually**

Run: `cargo run -p anno-rag-bin -- vault status`
Expected: JSON output with `"keyring_entry_present": true` (after first prior run) or `false` (clean dev machine). Either is correct; the test is that the command returns successfully and emits valid JSON without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-bin/src/main.rs crates/anno-rag/tests/vault_admin.rs
git commit -m "feat(anno-rag-bin): add 'vault status' and 'vault rotate' subcommands"
```

---

## Task 9: Create `claude-for-legal/engine-compat.json`

**Files:**
- Create: `claude-for-legal/engine-compat.json`

- [ ] **Step 1: Write the file**

Determine the current `anno-rag` crate version from the workspace (currently `0.2.0` per `crates/anno-rag/Cargo.toml`). Set the minimum to the version that ships `anno_health` — that is, the version this plan produces. Pick `0.3.0` if the team is following semver and adding a new tool counts as minor; pick the next patch otherwise. Coordinate with the team. For the purposes of this file, use `0.3.0`.

```json
{
  "min_engine_version": "0.3.0",
  "recommended_engine_version": "0.3.0",
  "required_tools": ["search", "rehydrate", "detect", "vault_stats", "anno_health"],
  "release_page_url": "https://github.com/arclabs561/anno/releases"
}
```

- [ ] **Step 2: Commit**

```bash
git add claude-for-legal/engine-compat.json
git commit -m "feat(claude-for-legal): declare min/recommended anno engine versions"
```

---

## Task 10: Create `anno-engine-check` skill

**Files:**
- Create: `claude-for-legal/skills/anno-engine-check/SKILL.md`

- [ ] **Step 1: Write the skill**

```markdown
---
name: anno-engine-check
description: >
  Verify the installed anno-rag engine satisfies this plugin's version and
  tool requirements before any other anno tool is called. Reads
  `engine-compat.json` from the plugin root, calls `anno_health` on the MCP
  server, and routes any drift into a one-line agent reply (notice or
  blocker). Invoked as the first step of every practice-area skill that
  touches anno tools.
argument-hint: ""
---

# /anno-engine-check

This skill is **not** a hook. It is invoked explicitly by practice-area
skills as their first step. Pattern matches `auto-updater`: explicit,
user-visible, audit-friendly.

## Steps

1. **Load `engine-compat.json`** from the active plugin's root directory.
   Required fields: `min_engine_version`, `recommended_engine_version`,
   `required_tools` (array), `release_page_url`.

   If the file is missing, treat as `min = recommended = "0.0.0"`,
   `required_tools = []`, `release_page_url = "(unknown)"`. Log a
   one-line debug notice but do not block.

2. **Call `anno_health`** on the connected anno MCP server. Parse the JSON
   into `{engine_version, build_target, signed, extension_install,
   vault_initialized, available_tools}`.

   If the MCP call fails, emit the **MCP unreachable blocker** below and abort.

3. **Semver-compare `engine_version` against `min_engine_version`** and
   `recommended_engine_version`. Use lexicographic semver (treat
   `pre-release` and `build-metadata` per semver 2.0.0).

4. **Compute `missing_tools = required_tools - available_tools`** (set
   difference, exact string match).

5. **Route into one of these outcomes (first match wins):**

   | Condition | Reply |
   |---|---|
   | `engine < min` | **Blocker.** "anno engine `{engine_version}` is below the minimum `{min_engine_version}` required by `{plugin}`. Download a newer engine from {release_page_url} and reinstall." → Abort caller. |
   | `missing_tools` not empty | **Blocker.** "anno engine `{engine_version}` is missing required tool(s): `{joined}`. Update the engine from {release_page_url}." → Abort caller. |
   | `engine_version` MCP call failed | **MCP unreachable blocker.** "anno engine is not running or not reachable. Open Claude Desktop → Settings → Connectors and verify `anno-rag` is enabled, or reinstall the Hacienda extension from {release_page_url}." → Abort caller. |
   | `min ≤ engine < recommended` | **Yellow notice.** "anno engine `{engine_version}` works for this skill but `{recommended_engine_version}` is available at {release_page_url}." → Proceed. |
   | `signed = false` AND `extension_install = false` AND we are NOT in a dev session | **Yellow notice.** "anno engine is a development build. Outputs are not suitable for production legal work." → Proceed. |
   | All checks pass | Silent. → Proceed. |

## Forward-compat contract

This skill MUST proceed silently when `engine > recommended` (engines are
backward-compatible across minor versions within a major per spec §13.4).
Never block on "engine too new."

## What this skill does NOT do

- It does **not** open the vault, ingest documents, or load models.
- It does **not** install or update the engine — only notifies.
- It does **not** retry on transient failures — the calling skill decides
  retry strategy.

## Runbook

See [docs/runbooks/anno-engine-check.md](../../docs/runbooks/anno-engine-check.md)
for diagnostic flows when this skill emits a blocker.
```

- [ ] **Step 2: Commit**

```bash
git add claude-for-legal/skills/anno-engine-check/SKILL.md
git commit -m "feat(claude-for-legal): add anno-engine-check skill (spec §13.3)"
```

---

## Task 11: Reference the skill from `legal-builder-hub/CLAUDE.md`

**Files:**
- Modify: `claude-for-legal/legal-builder-hub/CLAUDE.md`

- [ ] **Step 1: Inspect the current CLAUDE.md structure**

Run: `cat claude-for-legal/legal-builder-hub/CLAUDE.md | head -60`
Look for a section that registers helper skills (likely under a "Skills" heading or "Shared skills" or similar).

- [ ] **Step 2: Add a "Pre-anno-tool check" section**

In `claude-for-legal/legal-builder-hub/CLAUDE.md`, append (or insert near other skill references):

```markdown
## Pre-anno-tool check (mandatory)

Every practice-area skill that calls anno tools (`/search`, `/rehydrate`,
`/detect`, etc.) MUST invoke `/anno-engine-check` as its first step. The
check reads `claude-for-legal/engine-compat.json` and verifies the
installed engine version + tool surface. If the check emits a blocker,
the practice-area skill aborts before any anno call.

This is not enforced by hooks (the plugin currently has `hooks.json` set
to `{}`) — it is enforced by skill authors. New skills must add the check;
reviewers must confirm its presence.
```

- [ ] **Step 3: Commit**

```bash
git add claude-for-legal/legal-builder-hub/CLAUDE.md
git commit -m "docs(claude-for-legal): require anno-engine-check before anno tool calls"
```

---

## Task 12: Write the runbook

**Files:**
- Create: `docs/runbooks/anno-engine-check.md`

- [ ] **Step 1: Write the runbook**

```markdown
# anno-engine-check runbook

Diagnostic flows for the `claude-for-legal/skills/anno-engine-check`
skill (spec §13.3).

## Triage table

| Blocker text the user saw | Likely cause | Next step |
|---|---|---|
| "anno engine `X.Y.Z` is below the minimum…" | User upgraded the plugin but not the engine. | Download the engine artifact for their OS from the release page and reinstall. |
| "anno engine is missing required tool(s): …" | Plugin expects a tool the engine does not yet expose. Usually a plugin-too-new condition. | Same: upgrade the engine. Confirm `anno_health.available_tools` after the upgrade. |
| "anno engine is not running or not reachable." | (a) Claude Desktop has not loaded the connector; (b) `anno-rag` binary missing or quarantined by AV; (c) `claude_desktop_config.json` entry corrupt. | Open Claude Desktop → Settings → Connectors. Confirm `anno-rag` is enabled and shows healthy. If absent, reinstall the Hacienda extension. If AV quarantined, ask IT to whitelist the engine path. |
| "anno engine is a development build…" | User installed a non-CI build. | Replace with a release build from the official release page. Acceptable to ignore for dev environments. |

## Verifying engine health manually

From a terminal:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"anno_health","arguments":{}}}' | anno-rag mcp
```

The response JSON should contain `engine_version`, `available_tools`, and
`vault_initialized`. Compare against `claude-for-legal/engine-compat.json`.

## Verifying via CLI

```bash
anno-rag vault status
```

Reports keyring entry presence. Output does not include the key itself.

## When the plugin says "engine too old" but `anno-rag --version` says it is current

Two engines can be installed: one via the `.mcpb` extension (under
Claude Desktop's extension dir) and one manually on PATH. Claude Desktop
launches the extension copy, not the PATH copy. Check the extension's
installed version under Claude Desktop → Settings → Extensions.

## Escalation

If `anno_health` returns successfully but reports a version older than
the current release on GitHub, file an issue with:

- The full `anno_health` JSON output.
- The path Claude Desktop used to launch the engine (from
  `claude_desktop_config.json` mcpServers.anno-rag.command).
- The output of `anno-rag --version` from PATH.
- OS and Claude Desktop version.
```

- [ ] **Step 2: Commit**

```bash
git add docs/runbooks/anno-engine-check.md
git commit -m "docs(runbook): anno-engine-check diagnostic flows"
```

---

## Task 13: End-to-end integration check

**Files:** none — verification only.

- [ ] **Step 1: Build the workspace cleanly**

Run: `cargo build --workspace`
Expected: clean build, no warnings on new files.

- [ ] **Step 2: Run the full anno-rag and anno-rag-mcp test suites**

Run: `cargo test -p anno-rag -p anno-rag-mcp`
Expected: all PASS.

- [ ] **Step 3: Smoke-test the new CLI subcommands**

Run: `cargo run -p anno-rag-bin -- vault status`
Expected: JSON with `service`, `account`, `keyring_entry_present`. No passphrase substring in output.

Run: `cargo run -p anno-rag-bin -- --help`
Expected: `vault` listed alongside `ingest`, `search`, `mcp`, `bench`.

- [ ] **Step 4: Verify the `anno_health` tool via MCP stdio**

Start the server interactively:

Run: `cargo run -p anno-rag-bin -- mcp`

In another terminal (or via Claude Desktop with the connector enabled), invoke `anno_health`. Confirm the response JSON contains `engine_version` matching `cargo metadata --no-deps | jq '.packages[] | select(.name=="anno-rag") | .version'`.

- [ ] **Step 5: Verify `gitnexus_detect_changes` reflects only expected scope**

Run: `gitnexus_detect_changes({scope: "staged"})` (per project CLAUDE.md guidance).
Expected: changes confined to `crates/anno-rag/src/{lib.rs,vault.rs,vault_admin.rs}`, `crates/anno-rag/tests/vault_admin.rs`, `crates/anno-rag-bin/src/main.rs`, `crates/anno-rag-mcp/src/{lib.rs,health.rs}`, `crates/anno-rag-mcp/tests/health.rs`, `claude-for-legal/{engine-compat.json,skills/anno-engine-check/SKILL.md,legal-builder-hub/CLAUDE.md}`, `docs/runbooks/anno-engine-check.md`, `docs/superpowers/specs/2026-05-18-github-release-binaries-and-mcp-distribution-design.md`. Nothing else.

- [ ] **Step 6: Refresh the GitNexus index after final commit**

Run: `npx gitnexus analyze --embeddings` (per project CLAUDE.md "Keeping the Index Fresh").
Expected: clean index, no errors.

---

## Self-review checklist (run before declaring the plan done)

- [ ] Spec §13 (Engine ↔ plugin version compat) — all sub-sections (`anno_health` tool §13.1, `engine-compat.json` §13.2, `anno-engine-check` skill §13.3, forward-compat §13.4) have at least one task.
- [ ] Spec §14 — Task 0 reconciled the spec; Tasks 4–8 implement Path B + status + rotate. §14.3 export-recovery is **intentionally deferred**, not lost.
- [ ] Phase E from spec §10 — fully covered: `anno_health` (Task 3), `engine-compat.json` (Task 9), `anno-engine-check` skill (Task 10), skill-not-hook explicit (Task 10 + Task 11).
- [ ] No `TBD`/`TODO`/`placeholder` strings in any task step.
- [ ] Every code-touching step has actual code.
- [ ] Type names consistent: `EngineHealth`, `InitVaultParams`, `InitVaultResult`, `VaultStatus`, `VaultCmd` — all defined exactly once, referenced consistently in later tasks.
- [ ] Every task ends with a commit.
- [ ] No task assumes Phase C (`.mcpb` extension) or Phase D (gateway setup) has shipped. Plan 2 and Plan 3 are downstream and independent.

---

## Out of scope for this plan (Plan 2 / Plan 3 follow-ups)

- `.mcpb` packaging, signing, and `setup_required` MCP error variants tied to extension install — **Plan 2 (Phase C)**.
- `anno-privacy-gateway register --auto`, `configure`, `/healthz`, and the `anno-gateway-setup` skill — **Plan 3 (Phase D)**.
- `anno-rag vault export-recovery` subcommand — deferred to a follow-up.
- Re-encryption of the on-disk vault during rotation — deferred (rotation currently only rotates the keyring entry; a follow-up adds full re-encryption).
- Multi-profession generalization of `claude-for-legal` — deferred to a sibling spec.
