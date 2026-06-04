# Cross-Platform MCP Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a simple cross-platform setup flow that installs or points to `anno-rag`, downloads the local model cache, and configures Claude Desktop/Cowork plus Claude Code for local MCP.

**Architecture:** Add a focused `setup_mcp` module to the `anno-rag-bin` binary crate. Keep JSON config mutation in Rust with `serde_json`, keep OS download/extract concerns in thin PowerShell/Bash wrappers, and keep MCP validation as a fast stdio health smoke that does not index user folders.

**Tech Stack:** Rust 2021, Clap, serde_json, Tokio process I/O, PowerShell, Bash, existing `anno-rag download-models`, existing GitHub Release assets, existing `scripts/dev-fast.ps1` targeted build loop.

**Execution Result (2026-06-04):** Implemented. The final local verification
passed `cargo test -p anno-rag-bin setup_mcp`, `cargo test -p anno-rag-bin
parses_setup_mcp_command`, `powershell -NoProfile -ExecutionPolicy Bypass -File
scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check`, and
`scripts\setup-mcp.ps1 -Source local-build -Target manual -SkipModels -DryRun`.

---

## Scope Check

This plan implements one product flow from the approved spec:

- Spec: `docs/superpowers/specs/2026-06-04-cross-platform-mcp-setup-design.md`
- `desktop` means Claude Desktop plus Cowork running in Claude Desktop.
- `claude-code` means Claude Code CLI configuration through `claude mcp add`.
- `all` means both local client targets.

Do not implement remote/cloud MCP connectors in this plan.

Use a clean branch or worktree for implementation. The current workspace may contain unrelated deleted files under `claude-for-legal/` and `proptest-regressions/`; do not revert or stage them unless the user explicitly asks.

## File Map

Create:

- `crates/anno-rag-bin/src/setup_mcp.rs` - CLI args, config merge logic, model setup orchestration, Claude Code command building, and fast MCP smoke.
- `scripts/setup-mcp.ps1` - Windows wrapper for release/local/path setup.
- `scripts/setup-mcp.sh` - macOS/Linux wrapper for release/local/path setup.

Modify:

- `crates/anno-rag-bin/Cargo.toml` - add `tempfile` as a dev-dependency for setup config write tests.
- `crates/anno-rag-bin/src/main.rs` - add the `setup-mcp` subcommand and route it before `Pipeline::new`.
- `scripts/release/package-windows.ps1` - include `scripts/setup-mcp.ps1` and `scripts/setup-mcp.sh` in release archives.
- `scripts/release/package-unix.sh` - include setup scripts in release archives.
- `docs/getting-started/claude-desktop-cowork.md` - replace future-tense setup wording after implementation.
- `docs/release/README-release.md` - make `setup-mcp` the primary release install path after implementation.
- `docs/reference/commands.md` - document `anno-rag setup-mcp`.

Do not modify:

- MCP tool schemas.
- Corpus scoping logic.
- Vault storage semantics.
- Model inference code.

## Build And Test Rules

Use targeted commands only:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check
cargo test -p anno-rag-bin setup_mcp
```

Do not run `cargo build --workspace`, `cargo test --workspace`, or local release builds for this feature.

---

### Task 0: Pre-Flight Impact And Baseline

**Files:** none

- [ ] **Step 1: Verify GitNexus status**

Run:

```powershell
npx gitnexus status --repo anno
```

Expected: the index is usable. If stale, run `npx gitnexus analyze --repo anno` before code edits and avoid committing generated metadata churn unless required.

- [ ] **Step 2: Run impact checks for edited symbols**

Run:

```powershell
npx gitnexus impact --repo anno Cmd --direction upstream
npx gitnexus impact --repo anno main --direction upstream
npx gitnexus impact --repo anno download --direction upstream
```

Expected: record the risk level before editing. If HIGH or CRITICAL appears, tell the user before proceeding.

- [ ] **Step 3: Verify the current binary parses**

Run:

```powershell
cargo test -p anno-rag-bin parses_diagnose_gpu_command
```

Expected: PASS.

---

### Task 1: Add Setup CLI Types And Pure Helpers

**Files:**
- Create: `crates/anno-rag-bin/src/setup_mcp.rs`
- Modify: `crates/anno-rag-bin/Cargo.toml`
- Modify: `crates/anno-rag-bin/src/main.rs` (module declaration only)

- [ ] **Step 1: Add dev dependency**

Add this block to `crates/anno-rag-bin/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

Add the module declaration to `crates/anno-rag-bin/src/main.rs` so the new file's tests are compiled:

```rust
mod setup_mcp;
```

- [ ] **Step 2: Write failing helper tests**

Create `crates/anno-rag-bin/src/setup_mcp.rs` with the test module first:

```rust
use clap::{Args, ValueEnum};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_all_includes_desktop_and_claude_code() {
        assert!(SetupTarget::All.includes_desktop());
        assert!(SetupTarget::All.includes_claude_code());
        assert!(SetupTarget::Desktop.includes_desktop());
        assert!(!SetupTarget::Desktop.includes_claude_code());
        assert!(SetupTarget::ClaudeCode.includes_claude_code());
        assert!(!SetupTarget::ClaudeCode.includes_desktop());
        assert!(!SetupTarget::Manual.includes_desktop());
        assert!(!SetupTarget::Manual.includes_claude_code());
    }

    #[test]
    fn default_models_dir_ends_with_models() {
        let path = default_models_dir();
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("models"));
    }

    #[test]
    fn validates_absolute_binary_path() {
        let err = validate_absolute_path(Path::new("relative/anno-rag")).unwrap_err();
        assert!(err.contains("absolute"));
    }
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: FAIL because `SetupTarget`, `default_models_dir`, and `validate_absolute_path` do not exist.

- [ ] **Step 4: Implement CLI types and helpers**

Add these definitions above the test module in `crates/anno-rag-bin/src/setup_mcp.rs`:

```rust
use anno_rag::AnnoRagConfig;
use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Args)]
pub struct SetupMcpArgs {
    #[arg(long, value_enum, default_value_t = SetupTarget::All)]
    pub target: SetupTarget,
    #[arg(long, value_name = "PATH")]
    pub binary: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub models_dir: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub desktop_config: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = DesktopMode::Json)]
    pub desktop_mode: DesktopMode,
    #[arg(long, value_enum, default_value_t = ClaudeCodeScope::User)]
    pub claude_code_scope: ClaudeCodeScope,
    #[arg(long, default_value_t = false)]
    pub skip_models: bool,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SetupTarget {
    Desktop,
    ClaudeCode,
    All,
    Manual,
}

impl SetupTarget {
    pub fn includes_desktop(self) -> bool {
        matches!(self, Self::Desktop | Self::All)
    }

    pub fn includes_claude_code(self) -> bool {
        matches!(self, Self::ClaudeCode | Self::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DesktopMode {
    Json,
    Mcpb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClaudeCodeScope {
    Local,
    User,
    Project,
}

impl ClaudeCodeScope {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

pub fn default_models_dir() -> PathBuf {
    AnnoRagConfig::default().models_cache()
}

pub fn validate_absolute_path(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(format!("path must be absolute: {}", path.display()))
    }
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: PASS for the new helper tests.

- [ ] **Step 6: Commit**

```powershell
git add crates\anno-rag-bin\Cargo.toml crates\anno-rag-bin\src\main.rs crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: add mcp setup cli helpers"
```

---

### Task 2: Add Desktop/Cowork JSON Merge

**Files:**
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Write failing merge tests**

Append to the existing test module:

```rust
    #[test]
    fn desktop_merge_preserves_existing_servers_and_omits_vault_secret() {
        let binary = std::env::temp_dir().join(if cfg!(windows) {
            "anno-rag.exe"
        } else {
            "anno-rag"
        });
        let models_dir = std::env::temp_dir().join("anno-rag-models");
        let filesystem_server = json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem"],
            "env": {
                "ROOT": "/safe/root"
            }
        });
        let existing = json!({
            "theme": "dark",
            "mcpServers": {
                "filesystem": filesystem_server.clone()
            }
        });
        let merged = merge_desktop_config(
            existing,
            &binary,
            &models_dir,
            true,
        )
        .expect("merge");

        assert_eq!(merged["theme"], "dark");
        assert_eq!(merged["mcpServers"]["filesystem"], filesystem_server);
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["command"],
            binary.to_str().expect("temp path is valid utf-8")
        );
        assert_eq!(merged["mcpServers"]["anno-rag"]["args"], json!(["mcp"]));
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["env"]["ANNO_MODELS_DIR"],
            models_dir.to_str().expect("temp path is valid utf-8")
        );
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["env"]["ANNO_NO_DOWNLOADS"],
            "1"
        );
        let env = merged["mcpServers"]["anno-rag"]["env"].as_object().expect("env object");
        assert!(!env.contains_key("ANNO_RAG_VAULT_PASSPHRASE"));
    }

    #[test]
    fn desktop_merge_replaces_existing_anno_rag_and_omits_no_downloads_when_models_unverified() {
        let binary = std::env::temp_dir().join(if cfg!(windows) {
            "anno-rag.exe"
        } else {
            "anno-rag"
        });
        let models_dir = std::env::temp_dir().join("anno-rag-models");
        let existing = json!({
            "mcpServers": {
                "anno-rag": {
                    "command": "/old/path/anno-rag",
                    "args": ["mcp"],
                    "env": {
                        "ANNO_MODELS_DIR": "/old/models",
                        "ANNO_NO_DOWNLOADS": "1",
                        "ANNO_RAG_VAULT_PASSPHRASE": "must-not-survive"
                    }
                }
            }
        });

        let merged = merge_desktop_config(existing, &binary, &models_dir, false).expect("merge");

        assert_eq!(
            merged["mcpServers"]["anno-rag"]["command"],
            binary.to_str().expect("temp path is valid utf-8")
        );
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["env"]["ANNO_MODELS_DIR"],
            models_dir.to_str().expect("temp path is valid utf-8")
        );
        let env = merged["mcpServers"]["anno-rag"]["env"].as_object().expect("env object");
        assert!(!env.contains_key("ANNO_NO_DOWNLOADS"));
        assert!(!env.contains_key("ANNO_RAG_VAULT_PASSPHRASE"));
    }

    #[test]
    fn desktop_merge_inserts_anno_rag_when_config_is_empty() {
        let binary = std::env::temp_dir().join(if cfg!(windows) {
            "anno-rag.exe"
        } else {
            "anno-rag"
        });
        let models_dir = std::env::temp_dir().join("anno-rag-models");

        let merged = merge_desktop_config(json!({}), &binary, &models_dir, false).expect("merge");

        assert!(merged["mcpServers"]["anno-rag"].is_object());
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["command"],
            binary.to_str().expect("temp path is valid utf-8")
        );
        assert_eq!(merged["mcpServers"]["anno-rag"]["args"], json!(["mcp"]));
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["env"]["ANNO_MODELS_DIR"],
            models_dir.to_str().expect("temp path is valid utf-8")
        );
        let env = merged["mcpServers"]["anno-rag"]["env"].as_object().expect("env object");
        assert!(!env.contains_key("ANNO_NO_DOWNLOADS"));
    }

    #[test]
    fn desktop_merge_rejects_non_object_root() {
        let binary = std::env::temp_dir().join(if cfg!(windows) {
            "anno-rag.exe"
        } else {
            "anno-rag"
        });
        let models_dir = std::env::temp_dir().join("anno-rag-models");

        let err = merge_desktop_config(json!([]), &binary, &models_dir, true).unwrap_err();
        assert!(err.to_string().contains("root"));
    }
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin desktop_merge_preserves_existing_servers_and_omits_vault_secret
```

Expected: FAIL because `merge_desktop_config` does not exist.

- [ ] **Step 3: Implement merge function**

Add this function outside the test module:

```rust
pub fn merge_desktop_config(
    mut existing: Value,
    binary: &Path,
    models_dir: &Path,
    models_verified: bool,
) -> anyhow::Result<Value> {
    validate_absolute_path(binary).map_err(|e| anyhow!(e))?;
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;

    let root = existing
        .as_object_mut()
        .context("desktop config root must be a JSON object")?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("mcpServers must be a JSON object")?;

    let mut env = serde_json::Map::new();
    env.insert(
        "ANNO_MODELS_DIR".to_string(),
        Value::String(path_to_config_string(models_dir)?),
    );
    if models_verified {
        env.insert("ANNO_NO_DOWNLOADS".to_string(), Value::String("1".to_string()));
    }

    servers.insert(
        "anno-rag".to_string(),
        json!({
            "command": path_to_config_string(binary)?,
            "args": ["mcp"],
            "env": env
        }),
    );

    Ok(existing)
}

fn path_to_config_string(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(str::to_string)
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: merge desktop mcp config safely"
```

---

### Task 3: Add Desktop Config Path, Backup, Atomic Write, And Dry Run

**Files:**
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Write failing file-write tests**

Append to the test module:

```rust
    #[test]
    fn dry_run_does_not_write_desktop_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        let merged = json!({"mcpServers":{"anno-rag":{"command":"C:/Tools/anno-rag.exe","args":["mcp"]}}});

        let result = write_desktop_config(&config_path, &merged, true).expect("dry run");
        assert!(!config_path.exists());
        assert!(!result.changed);
        assert!(result.message.contains("dry-run"));
    }

    #[test]
    fn write_desktop_config_creates_backup_on_second_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&config_path, "{\"mcpServers\":{}}").expect("seed");

        let merged = json!({"mcpServers":{"anno-rag":{"command":"C:/Tools/anno-rag.exe","args":["mcp"]}}});
        let result = write_desktop_config(&config_path, &merged, false).expect("write");

        assert!(result.changed);
        assert!(config_path.exists());
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak."))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn repeated_writes_create_distinct_backups() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&config_path, "{\"first\":true}").expect("seed");

        let second = json!({"second": true});
        let third = json!({"third": true});
        write_desktop_config(&config_path, &second, false).expect("second write");
        write_desktop_config(&config_path, &third, false).expect("third write");

        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak."))
            .collect();
        assert_eq!(backups.len(), 2);
    }

    #[test]
    fn unique_temp_paths_are_same_directory_and_distinct() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        let first = unique_temp_path(&config_path, 0);
        let second = unique_temp_path(&config_path, 1);

        assert_eq!(first.parent(), config_path.parent());
        assert_eq!(second.parent(), config_path.parent());
        assert_ne!(first, second);
        assert!(first.file_name().unwrap().to_string_lossy().contains(".tmp."));
        assert!(second.file_name().unwrap().to_string_lossy().contains(".tmp."));
    }

    #[cfg(unix)]
    #[test]
    fn write_desktop_config_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&config_path, "{\"mcpServers\":{}}").expect("seed");
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))
            .expect("set permissions");

        let merged = json!({"mcpServers":{"anno-rag":{"command":"/tmp/anno-rag","args":["mcp"]}}});
        write_desktop_config(&config_path, &merged, false).expect("write");

        let mode = std::fs::metadata(&config_path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin dry_run_does_not_write_desktop_config write_desktop_config_creates_backup_on_second_write
```

Expected: FAIL because `write_desktop_config` does not exist.

- [ ] **Step 3: Implement config path and write helpers**

Add these items outside the test module:

```rust
#[derive(Debug, Clone)]
pub struct WriteResult {
    pub changed: bool,
    pub message: String,
}

pub fn default_desktop_config_path() -> anyhow::Result<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .context("APPDATA is not set; pass --desktop-config")?;
        return Ok(appdata.join("Claude").join("claude_desktop_config.json"));
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set; pass --desktop-config")?;
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json"));
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        anyhow::bail!("Claude Desktop config auto-detection is supported on Windows and macOS only; pass --desktop-config for managed clients")
    }
}

pub fn read_json_file_or_empty(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn write_desktop_config(path: &Path, value: &Value, dry_run: bool) -> anyhow::Result<WriteResult> {
    if dry_run {
        return Ok(WriteResult {
            changed: false,
            message: format!("dry-run: would write {}", path.display()),
        });
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }

    let existing_permissions = if path.exists() {
        let permissions = std::fs::metadata(path)
            .with_context(|| format!("metadata {}", path.display()))?
            .permissions();
        let backup = create_backup(path)?;
        std::fs::copy(path, &backup)
            .with_context(|| format!("backup {} to {}", path.display(), backup.display()))?;
        Some(permissions)
    } else {
        None
    };

    let tmp = create_unique_temp_file(path, value, existing_permissions.as_ref())?;
    replace_file(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn create_backup(path: &Path) -> anyhow::Result<PathBuf> {
    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("claude_desktop_config.json");
    for attempt in 0..1000_u32 {
        let backup = path.with_file_name(format!(
            "{}.bak.{}.{}",
            base,
            chrono::Utc::now().format("%Y%m%d%H%M%S%f"),
            attempt
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
        {
            Ok(_) => return Ok(backup),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("create backup {}", backup.display())),
        }
    }
    anyhow::bail!("could not allocate unique backup path for {}", path.display())
}

fn create_unique_temp_file(
    path: &Path,
    value: &Value,
    permissions: Option<&std::fs::Permissions>,
) -> anyhow::Result<PathBuf> {
    let text = serde_json::to_string_pretty(value)?;
    for attempt in 0..1000_u32 {
        let tmp = unique_temp_path(path, attempt);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(text.as_bytes())
                    .with_context(|| format!("write {}", tmp.display()))?;
                file.sync_all()
                    .with_context(|| format!("sync {}", tmp.display()))?;
                drop(file);
                if let Some(perms) = permissions {
                    std::fs::set_permissions(&tmp, perms.clone())
                        .with_context(|| format!("set permissions {}", tmp.display()))?;
                }
                return Ok(tmp);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("create temp {}", tmp.display())),
        }
    }
    anyhow::bail!("could not allocate unique temp path for {}", path.display())
}

fn unique_temp_path(path: &Path, attempt: u32) -> PathBuf {
    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("claude_desktop_config.json");
    path.with_file_name(format!(
        ".{}.tmp.{}.{}",
        base,
        chrono::Utc::now().format("%Y%m%d%H%M%S%f"),
        attempt
    ))
}

fn finalize_write_message(path: &Path) -> WriteResult {
    WriteResult {
        changed: true,
        message: format!("wrote {}", path.display()),
    }
}

// Keep the existing Windows `MoveFileExW` implementation and the non-Windows
// `std::fs::rename` implementation, but have the platform-specific helpers
// return `anyhow::Result<()>`. The shared `replace_file` wrapper should return
// the common `WriteResult` message after the platform-specific replacement
// succeeds.
```

The plan snippet above sketches the helper responsibilities. Keep the existing
Windows `MoveFileExW` body and make it return `anyhow::Result<()>` so the shared
`replace_file` wrapper can handle the common success message and temp-file
cleanup.

Important safety requirements:

- Backup path allocation must use `create_new(true)` so a backup is never
  overwritten.
- Temp path allocation must use `create_new(true)` so concurrent runs do not
  clobber each other.
- Temp file must live in the same directory as the target for same-filesystem
  replacement.
- On replacement failure, remove the temp file.
- When replacing an existing config, preserve the target file permissions. On
  Windows, use `ReplaceFileW` rather than `MoveFileExW` so NTFS security
  metadata is preserved by the replacement API. On Unix, set the temp file's
  permissions to the existing target permissions before `rename`.

Previous simple implementation:

```rust
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))?;

    Ok(WriteResult {
        changed: true,
        message: format!("wrote {}", path.display()),
    })
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: write desktop mcp config atomically"
```

---

### Task 4: Add Claude Code Command Builder And Execution

**Files:**
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Write failing Claude Code tests**

Append to the test module:

```rust
    #[test]
    fn claude_code_args_include_scope_env_binary_and_mcp() {
        let args = build_claude_code_args(
            ClaudeCodeScope::User,
            Path::new("C:/Users/you/.anno-rag/models"),
            Path::new("C:/Tools/hacienda/anno-rag.exe"),
        )
        .expect("args");

        assert_eq!(args[0], "mcp");
        assert_eq!(args[1], "add");
        assert!(args.contains(&"--transport".to_string()));
        assert!(args.contains(&"stdio".to_string()));
        assert!(args.contains(&"--scope".to_string()));
        assert!(args.contains(&"user".to_string()));
        assert!(args.contains(&"--env".to_string()));
        assert!(args.contains(&"ANNO_MODELS_DIR=C:/Users/you/.anno-rag/models".to_string()));
        assert!(args.contains(&"anno-rag".to_string()));
        assert!(args.contains(&"mcp".to_string()));
    }
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin claude_code_args_include_scope_env_binary_and_mcp
```

Expected: FAIL because `build_claude_code_args` does not exist.

- [ ] **Step 3: Implement command builder and runner**

Add these functions outside the test module:

```rust
pub fn build_claude_code_args(
    scope: ClaudeCodeScope,
    models_dir: &Path,
    binary: &Path,
) -> anyhow::Result<Vec<String>> {
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    validate_absolute_path(binary).map_err(|e| anyhow!(e))?;
    Ok(vec![
        "mcp".to_string(),
        "add".to_string(),
        "--transport".to_string(),
        "stdio".to_string(),
        "--scope".to_string(),
        scope.as_cli_value().to_string(),
        "--env".to_string(),
        format!("ANNO_MODELS_DIR={}", models_dir.display()),
        "anno-rag".to_string(),
        "--".to_string(),
        binary.display().to_string(),
        "mcp".to_string(),
    ])
}

pub fn configure_claude_code(
    scope: ClaudeCodeScope,
    models_dir: &Path,
    binary: &Path,
    dry_run: bool,
) -> anyhow::Result<String> {
    let args = build_claude_code_args(scope, models_dir, binary)?;
    if dry_run {
        return Ok(format!("dry-run: claude {}", args.join(" ")));
    }

    let output = std::process::Command::new("claude")
        .args(&args)
        .output();

    match output {
        Ok(out) if out.status.success() => Ok("configured Claude Code MCP server anno-rag".to_string()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("claude mcp add failed: {stderr}");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(format!(
            "Claude Code CLI not found; run later: claude {}",
            args.join(" ")
        )),
        Err(e) => Err(e).context("run claude mcp add"),
    }
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: configure claude code mcp target"
```

---

### Task 5: Add Model Cache Verification And Download Orchestration

**Files:**
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Write failing model cache tests**

Append to the test module:

```rust
    #[test]
    fn model_cache_verified_when_expected_families_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        std::fs::create_dir_all(models.join("multilingual-e5-small")).expect("e5");
        std::fs::create_dir_all(models.join("gliner2-multi-v1-onnx")).expect("gliner");
        std::fs::write(models.join("multilingual-e5-small").join("config.json"), "{}").expect("e5 file");
        std::fs::write(models.join("gliner2-multi-v1-onnx").join("model.onnx"), "x").expect("gliner file");

        assert!(model_cache_verified(&models));
    }

    #[test]
    fn model_cache_not_verified_when_gliner_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        std::fs::create_dir_all(models.join("multilingual-e5-small")).expect("e5");
        std::fs::write(models.join("multilingual-e5-small").join("config.json"), "{}").expect("e5 file");

        assert!(!model_cache_verified(&models));
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin model_cache_verified_when_expected_families_exist model_cache_not_verified_when_gliner_missing
```

Expected: FAIL because `model_cache_verified` does not exist.

- [ ] **Step 3: Implement verification and download**

Add these functions outside the test module:

```rust
pub fn model_cache_verified(models_dir: &Path) -> bool {
    let e5 = models_dir.join("multilingual-e5-small");
    let gliner = models_dir.join("gliner2-multi-v1-onnx");
    directory_has_file(&e5) && directory_has_file(&gliner)
}

fn directory_has_file(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .any(|entry| entry.path().is_file())
}

pub async fn ensure_models(models_dir: &Path, skip_models: bool, dry_run: bool) -> anyhow::Result<bool> {
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    if model_cache_verified(models_dir) {
        return Ok(true);
    }
    if skip_models || dry_run {
        return Ok(false);
    }
    if models_dir.file_name().and_then(|s| s.to_str()) != Some("models") {
        anyhow::bail!("--models-dir must end with 'models' for anno-rag download-models compatibility");
    }
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = models_dir
        .parent()
        .map(Path::to_path_buf)
        .context("--models-dir must have a parent directory")?;
    anno_rag::download_models::download(&cfg).await?;
    Ok(model_cache_verified(models_dir))
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: verify setup model cache"
```

---

### Task 6: Add Fast MCP Health Smoke

**Files:**
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Write failing smoke message tests**

Append to the test module:

```rust
    #[test]
    fn initialize_payload_is_jsonrpc_line() {
        let line = jsonrpc_line(1, "initialize", json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "anno-setup", "version": "1.0"}
        }));
        assert!(line.ends_with('\n'));
        let parsed: Value = serde_json::from_str(line.trim()).expect("json");
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "initialize");
    }
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin initialize_payload_is_jsonrpc_line
```

Expected: FAIL because `jsonrpc_line` does not exist.

- [ ] **Step 3: Implement JSON-RPC helpers and smoke**

Add these functions outside the test module:

```rust
pub fn jsonrpc_line(id: u64, method: &str, params: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    }))
    .expect("jsonrpc payload serializes")
        + "\n"
}

pub fn notification_line(method: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": method
    }))
    .expect("jsonrpc notification serializes")
        + "\n"
}

pub async fn run_fast_mcp_smoke(binary: &Path, models_dir: &Path, dry_run: bool) -> anyhow::Result<String> {
    validate_absolute_path(binary).map_err(|e| anyhow!(e))?;
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    if dry_run {
        return Ok(format!("dry-run: would smoke-test {} mcp", binary.display()));
    }

    let mut child = tokio::process::Command::new(binary)
        .arg("mcp")
        .env("ANNO_MODELS_DIR", models_dir)
        .env("ANNO_RAG_VAULT_PASSPHRASE", "anno-setup-smoke-passphrase")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {} mcp", binary.display()))?;

    let mut stdin = child.stdin.take().context("open mcp stdin")?;
    let stdout = child.stdout.take().context("open mcp stdout")?;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut lines = BufReader::new(stdout).lines();

    stdin
        .write_all(jsonrpc_line(1, "initialize", json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "anno-setup", "version": "1.0"}
        })).as_bytes())
        .await?;
    let init = lines.next_line().await?.unwrap_or_default();
    if !init.contains("\"id\":1") {
        anyhow::bail!("MCP initialize failed: {init}");
    }

    stdin
        .write_all(notification_line("notifications/initialized").as_bytes())
        .await?;
    stdin
        .write_all(jsonrpc_line(2, "tools/list", json!({})).as_bytes())
        .await?;
    let tools = lines.next_line().await?.unwrap_or_default();
    if !tools.contains("anno_health") {
        anyhow::bail!("MCP tools/list did not include anno_health");
    }

    let _ = child.kill().await;
    Ok("fast MCP smoke passed".to_string())
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: add fast mcp setup smoke"
```

---

### Task 7: Wire `anno-rag setup-mcp`

**Files:**
- Modify: `crates/anno-rag-bin/src/main.rs`
- Modify: `crates/anno-rag-bin/src/setup_mcp.rs`

- [ ] **Step 1: Write failing CLI parse test**

In `crates/anno-rag-bin/src/main.rs`, add to `gpu_cli_tests` or a new `setup_mcp_cli_tests` module:

```rust
    #[test]
    fn parses_setup_mcp_command() {
        let cli = Cli::try_parse_from([
            "anno-rag",
            "setup-mcp",
            "--target",
            "manual",
            "--binary",
            "C:/Tools/hacienda/anno-rag.exe",
        ])
        .expect("parse");
        assert!(matches!(cli.cmd, Cmd::SetupMcp(_)));
    }
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag-bin parses_setup_mcp_command
```

Expected: FAIL because `Cmd::SetupMcp` is not defined.

- [ ] **Step 3: Wire command**

Add this variant to `Cmd`:

```rust
    /// Configure local MCP clients for Claude Desktop/Cowork and Claude Code.
    SetupMcp(setup_mcp::SetupMcpArgs),
```

Before the `DownloadModels` branch and before `Pipeline::new`, add:

```rust
    if matches!(&cli.cmd, Cmd::SetupMcp(_)) {
        let Cmd::SetupMcp(args) = cli.cmd else {
            unreachable!()
        };
        setup_mcp::run(args).await?;
        return Ok(());
    }
```

Add `Cmd::SetupMcp(_)` to the final `unreachable!` match.

- [ ] **Step 4: Implement `run` orchestration**

Add this function in `setup_mcp.rs`:

```rust
pub async fn run(args: SetupMcpArgs) -> anyhow::Result<()> {
    let binary = match args.binary {
        Some(path) => path,
        None => std::env::current_exe().context("resolve current executable")?,
    };
    validate_absolute_path(&binary).map_err(|e| anyhow!(e))?;

    let models_dir = args.models_dir.unwrap_or_else(default_models_dir);
    validate_absolute_path(&models_dir).map_err(|e| anyhow!(e))?;
    let models_verified = ensure_models(&models_dir, args.skip_models, args.dry_run).await?;

    let mut summary = Vec::<String>::new();
    summary.push(format!("binary: {}", binary.display()));
    summary.push(format!("models_dir: {}", models_dir.display()));
    summary.push(format!("models_verified: {models_verified}"));

    if args.target == SetupTarget::Manual {
        let desktop = merge_desktop_config(json!({}), &binary, &models_dir, models_verified)?;
        summary.push(format!(
            "desktop_json: {}",
            serde_json::to_string_pretty(&desktop)?
        ));
        summary.push(format!(
            "claude_code_command: claude {}",
            build_claude_code_args(args.claude_code_scope, &models_dir, &binary)?.join(" ")
        ));
        println!("{}", summary.join("\n"));
        return Ok(());
    }

    if args.target.includes_desktop() {
        if args.desktop_mode == DesktopMode::Mcpb {
            summary.push("desktop: .mcpb install is interactive; use the release .mcpb asset or rerun with --desktop-mode json".to_string());
        } else {
            let config_path = match args.desktop_config {
                Some(path) => path,
                None => default_desktop_config_path()?,
            };
            let existing = read_json_file_or_empty(&config_path)?;
            let merged = merge_desktop_config(existing, &binary, &models_dir, models_verified)?;
            let result = write_desktop_config(&config_path, &merged, args.dry_run)?;
            summary.push(format!("desktop: {}", result.message));
        }
    }

    if args.target.includes_claude_code() {
        let result = configure_claude_code(args.claude_code_scope, &models_dir, &binary, args.dry_run)?;
        summary.push(format!("claude_code: {result}"));
    }

    if !args.dry_run {
        match run_fast_mcp_smoke(&binary, &models_dir, false).await {
            Ok(msg) => summary.push(format!("smoke: {msg}")),
            Err(e) => summary.push(format!("smoke_warning: {e}")),
        }
    }

    summary.push("restart Claude Desktop/Cowork before verifying anno_health".to_string());
    println!("{}", summary.join("\n"));
    Ok(())
}
```

- [ ] **Step 5: Run tests and check**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
cargo test -p anno-rag-bin parses_setup_mcp_command
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add crates\anno-rag-bin\src\main.rs crates\anno-rag-bin\src\setup_mcp.rs
git commit -m "feat: add anno-rag setup-mcp command"
```

---

### Task 8: Add Windows Setup Wrapper

**Files:**
- Create: `scripts/setup-mcp.ps1`

- [ ] **Step 1: Create PowerShell wrapper**

Create `scripts/setup-mcp.ps1`:

```powershell
[CmdletBinding()]
param(
    [ValidateSet("desktop", "claude-code", "all", "manual")]
    [string]$Target = "all",
    [ValidateSet("release", "local-build", "path")]
    [string]$Source = "release",
    [string]$Tag = "latest",
    [string]$Binary,
    [string]$InstallDir = "$env:LOCALAPPDATA\anno-rag",
    [string]$ModelsDir = "$env:USERPROFILE\.anno-rag\models",
    [switch]$SkipModels,
    [switch]$DryRun,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-ReleaseTag {
    param([string]$RequestedTag)
    if ($RequestedTag -ne "latest") { return $RequestedTag }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/jamon8888/anno/releases/latest"
    return [string]$release.tag_name
}

function Resolve-LocalBuildBinary {
    $repoRoot = (& git rev-parse --show-toplevel).Trim()
    $candidate = Join-Path $repoRoot "target\debug\anno-rag.exe"
    if (-not (Test-Path -LiteralPath $candidate)) {
        powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\mcp-iterate.ps1") -Mode install -SkipCheck
        $candidate = Join-Path $env:LOCALAPPDATA "anno-rag\anno-rag.exe"
    }
    return (Resolve-Path -LiteralPath $candidate).Path
}

function Install-ReleaseBinary {
    param([string]$ResolvedTag)
    $target = "x86_64-pc-windows-msvc"
    $asset = "hacienda-$ResolvedTag-$target.zip"
    $base = "https://github.com/jamon8888/anno/releases/download/$ResolvedTag"
    $downloadDir = Join-Path $env:TEMP "anno-rag-$ResolvedTag"
    New-Item -ItemType Directory -Force -Path $downloadDir | Out-Null
    $assetPath = Join-Path $downloadDir $asset
    $sumsPath = Join-Path $downloadDir "SHA256SUMS.txt"
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $assetPath
    Invoke-WebRequest -Uri "$base/SHA256SUMS.txt" -OutFile $sumsPath
    $expected = (Select-String -Path $sumsPath -SimpleMatch $asset).Line.Split()[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $assetPath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { throw "checksum mismatch for $asset" }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Expand-Archive -Path $assetPath -DestinationPath $InstallDir -Force
    $exe = Get-ChildItem -Path $InstallDir -Recurse -File -Filter "anno-rag.exe" | Select-Object -First 1
    if (-not $exe) { throw "anno-rag.exe not found after extract" }
    return $exe.FullName
}

if ($Source -eq "path") {
    if (-not $Binary) { throw "-Binary is required when -Source path" }
    $ResolvedBinary = (Resolve-Path -LiteralPath $Binary).Path
} elseif ($Source -eq "local-build") {
    $ResolvedBinary = Resolve-LocalBuildBinary
} else {
    $ResolvedTag = Get-ReleaseTag -RequestedTag $Tag
    $ResolvedBinary = Install-ReleaseBinary -ResolvedTag $ResolvedTag
}

$args = @("setup-mcp", "--target", $Target, "--binary", $ResolvedBinary, "--models-dir", $ModelsDir)
if ($SkipModels) { $args += "--skip-models" }
if ($DryRun) { $args += "--dry-run" }
if ($Force) { $args += "--force" }

& $ResolvedBinary @args
exit $LASTEXITCODE
```

- [ ] **Step 2: Validate PowerShell parsing and dry-run path mode**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Source path -Binary "$PWD\target\debug\anno-rag.exe" -Target manual -SkipModels -DryRun
```

Expected: if the debug binary exists, the command prints manual config. If it does not exist, the script fails only because the binary path is missing, not because of syntax.

- [ ] **Step 3: Commit**

```powershell
git add scripts\setup-mcp.ps1
git commit -m "feat: add windows mcp setup wrapper"
```

---

### Task 9: Add macOS/Linux Setup Wrapper

**Files:**
- Create: `scripts/setup-mcp.sh`

- [ ] **Step 1: Create Bash wrapper**

Create `scripts/setup-mcp.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

target="all"
source="release"
tag="latest"
binary=""
install_dir="${HOME}/Tools/hacienda"
models_dir="${HOME}/.anno-rag/models"
skip_models=0
dry_run=0
force=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    --source) source="$2"; shift 2 ;;
    --tag) tag="$2"; shift 2 ;;
    --binary) binary="$2"; shift 2 ;;
    --install-dir) install_dir="$2"; shift 2 ;;
    --models-dir) models_dir="$2"; shift 2 ;;
    --skip-models) skip_models=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    --force) force=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

resolve_latest_tag() {
  if [[ "${tag}" != "latest" ]]; then
    printf '%s\n' "${tag}"
    return
  fi
  curl -fsSL https://api.github.com/repos/jamon8888/anno/releases/latest |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -n 1
}

detect_target() {
  local uname_s uname_m
  uname_s="$(uname -s)"
  uname_m="$(uname -m)"
  if [[ "${uname_s}" == "Darwin" && "${uname_m}" == "arm64" ]]; then
    printf 'aarch64-apple-darwin\n'
  elif [[ "${uname_s}" == "Darwin" && "${uname_m}" == "x86_64" ]]; then
    printf 'x86_64-apple-darwin\n'
  else
    echo "release install is currently supported by published macOS assets only; use --source path or --source local-build on ${uname_s}/${uname_m}" >&2
    exit 2
  fi
}

install_release_binary() {
  local resolved_tag target_triple asset base download_dir archive
  resolved_tag="$(resolve_latest_tag)"
  target_triple="$(detect_target)"
  asset="hacienda-${resolved_tag}-${target_triple}.tar.gz"
  base="https://github.com/jamon8888/anno/releases/download/${resolved_tag}"
  download_dir="${TMPDIR:-/tmp}/anno-rag-${resolved_tag}"
  mkdir -p "${download_dir}" "${install_dir}"
  archive="${download_dir}/${asset}"
  curl -fL "${base}/${asset}" -o "${archive}"
  curl -fL "${base}/SHA256SUMS.txt" -o "${download_dir}/SHA256SUMS.txt"
  expected="$(grep "${asset}" "${download_dir}/SHA256SUMS.txt" | awk '{print $1}')"
  actual="$(shasum -a 256 "${archive}" | awk '{print $1}')"
  test "${expected}" = "${actual}"
  tar -xzf "${archive}" -C "${install_dir}"
  find "${install_dir}" -type f -name anno-rag -perm -111 | head -n 1
}

if [[ "${source}" == "path" ]]; then
  if [[ -z "${binary}" ]]; then echo "--binary is required with --source path" >&2; exit 2; fi
  resolved_binary="$(cd "$(dirname "${binary}")" && pwd)/$(basename "${binary}")"
elif [[ "${source}" == "local-build" ]]; then
  repo_root="$(git rev-parse --show-toplevel)"
  cargo build -p anno-rag-bin --bin anno-rag
  resolved_binary="${repo_root}/target/debug/anno-rag"
else
  resolved_binary="$(install_release_binary)"
fi

args=(setup-mcp --target "${target}" --binary "${resolved_binary}" --models-dir "${models_dir}")
if [[ "${skip_models}" == "1" ]]; then args+=(--skip-models); fi
if [[ "${dry_run}" == "1" ]]; then args+=(--dry-run); fi
if [[ "${force}" == "1" ]]; then args+=(--force); fi

"${resolved_binary}" "${args[@]}"
```

- [ ] **Step 2: Mark executable**

Run:

```bash
chmod +x scripts/setup-mcp.sh
```

On Windows, run through Git Bash or WSL if available. If neither is available, ensure the executable bit is set in Git:

```powershell
git update-index --chmod=+x scripts/setup-mcp.sh
```

- [ ] **Step 3: Validate syntax**

Run:

```bash
bash -n scripts/setup-mcp.sh
```

Expected: exit code 0.

- [ ] **Step 4: Commit**

```powershell
git add scripts\setup-mcp.sh
git commit -m "feat: add unix mcp setup wrapper"
```

---

### Task 10: Include Setup Scripts In Release Archives

**Files:**
- Modify: `scripts/release/package-windows.ps1`
- Modify: `scripts/release/package-unix.sh`

- [ ] **Step 1: Update Windows required files**

In `scripts/release/package-windows.ps1`, add these entries to `$RequiredFiles`:

```powershell
    "scripts/setup-mcp.ps1",
    "scripts/setup-mcp.sh",
```

Create the scripts output directory next to `$ExamplesDir`:

```powershell
$ScriptsOutDir = Join-Path -Path $StagingDir -ChildPath "scripts"
```

After creating `$ExamplesDir`, also create `$ScriptsOutDir`:

```powershell
New-Item -ItemType Directory -Path $ScriptsOutDir -Force | Out-Null
```

In the existing `foreach ($RelativePath in $RequiredFiles)` loop, set the destination directory for setup scripts:

```powershell
    if ($RelativePath -like "scripts/setup-mcp.*") {
        $DestinationDir = $ScriptsOutDir
    }
```

- [ ] **Step 2: Update Unix required files**

In `scripts/release/package-unix.sh`, add these entries to `required_files`:

```bash
  "scripts/setup-mcp.ps1"
  "scripts/setup-mcp.sh"
```

Add these copy commands near the other `cp` calls:

```bash
mkdir -p "${staging_dir}/scripts"
cp -- "${repo_root}/scripts/setup-mcp.ps1" "${staging_dir}/scripts/"
cp -- "${repo_root}/scripts/setup-mcp.sh" "${staging_dir}/scripts/"
```

- [ ] **Step 3: Validate script syntax**

Run:

```powershell
powershell -NoProfile -Command "$null = [scriptblock]::Create((Get-Content -Raw scripts\release\package-windows.ps1)); 'ok'"
bash -n scripts/release/package-unix.sh
```

Expected: both commands exit 0.

- [ ] **Step 4: Commit**

```powershell
git add scripts\release\package-windows.ps1 scripts\release\package-unix.sh
git commit -m "chore: package mcp setup scripts"
```

---

### Task 11: Update Docs From Future-Tense To Available Command

**Files:**
- Modify: `docs/getting-started/claude-desktop-cowork.md`
- Modify: `docs/release/README-release.md`
- Modify: `docs/reference/commands.md`
- Modify: `README.md`

- [ ] **Step 1: Update the getting-started guide**

Replace the current future-tense setup paragraph with:

````markdown
The cross-platform setup helper is available through the release wrapper scripts
or the installed binary:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target all
```

```bash
./scripts/setup-mcp.sh --target all
```

The binary subcommand used by both wrappers is:

```bash
anno-rag setup-mcp --target all
```
````

- [ ] **Step 2: Update release README**

Make `setup-mcp` the primary path and keep manual JSON as fallback. The primary command block should be:

````markdown
Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target all -Tag latest
```

macOS:

```bash
./scripts/setup-mcp.sh --target all --tag latest
```
````

- [ ] **Step 3: Update commands reference**

Add this row to the common commands table in `docs/reference/commands.md`:

```markdown
| `anno-rag setup-mcp` | Configure local MCP clients for Claude Desktop/Cowork and Claude Code. | Use `--target all` for the normal local setup; use `--target manual --dry-run` to print config without writing files. |
```

- [ ] **Step 4: Run doc checks**

Run:

```powershell
git diff --check
rg -n "setup helper.*future|one-command.*designed|helper.*implemented" README.md docs -g "*.md" -g "!docs/superpowers/plans/**"
```

Expected: `git diff --check` exits 0. The `rg` command should not find stale future-tense wording for the setup command.

- [ ] **Step 5: Commit**

```powershell
git add README.md docs\getting-started\claude-desktop-cowork.md docs\release\README-release.md docs\reference\commands.md
git commit -m "docs: document mcp setup command"
```

---

### Task 12: Final Verification

**Files:** all modified files

- [ ] **Step 1: Run targeted unit tests**

Run:

```powershell
cargo test -p anno-rag-bin setup_mcp
cargo test -p anno-rag-bin parses_setup_mcp_command
```

Expected: PASS.

- [ ] **Step 2: Run targeted check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check
```

Expected: PASS.

- [ ] **Step 3: Run dry-run setup with local binary**

After the targeted check has produced or reused a debug binary, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Source local-build -Target manual -SkipModels -DryRun
```

Expected: output includes:

- `binary:`
- `models_dir:`
- `desktop_json:`
- `claude_code_command: claude mcp add`

- [ ] **Step 4: Run GitNexus change detection**

Run:

```powershell
npx gitnexus status --repo anno
npx gitnexus detect_changes --repo anno
```

If `detect_changes` is unavailable, run:

```powershell
npx gitnexus status --repo anno
git diff --stat HEAD
```

Expected: changed scope is limited to `anno-rag-bin`, setup scripts, release packaging scripts, and docs.

- [ ] **Step 5: Final commit if needed**

If Task 12 produced small verification/doc fixes:

```powershell
git add crates\anno-rag-bin scripts docs README.md
git commit -m "test: verify mcp setup flow"
```

---

## Acceptance Checklist

- [x] `anno-rag setup-mcp --target manual --dry-run` prints Desktop JSON and a Claude Code command.
- [x] `anno-rag setup-mcp --target desktop --dry-run` does not write config.
- [x] Desktop config merge preserves unrelated MCP servers.
- [x] Desktop config never writes `ANNO_RAG_VAULT_PASSPHRASE`.
- [x] Claude Code setup uses `claude mcp add --transport stdio --scope user` by default.
- [x] `--skip-models` avoids download and does not set `ANNO_NO_DOWNLOADS=1` unless cache is already verified.
- [x] Release wrappers support `release`, `local-build`, and `path` sources.
- [x] Release archives include setup scripts.
- [x] Targeted tests and `dev-fast.ps1 -Package anno-rag-bin -Mode check` pass.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-04-cross-platform-mcp-setup.md`.

Two execution options:

1. **Subagent-Driven (recommended)** - dispatch a fresh subagent per task, review between tasks, fastest isolation.
2. **Inline Execution** - execute tasks in this session using executing-plans, with checkpoints after Tasks 3, 7, and 12.
