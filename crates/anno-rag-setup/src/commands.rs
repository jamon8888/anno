use anyhow::Context;
use serde::{Deserialize as _, Serialize};
use std::path::PathBuf;
use tauri::ipc::Channel;

/// State returned to the UI after each wizard step.
#[derive(Debug, Serialize)]
pub struct StepResult {
    pub ok: bool,
    pub message: String,
}

/// Progress event sent over the download channel.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub pct: u8,
    pub current_file: String,
    pub downloaded_mb: f32,
    pub total_mb: f32,
}

/// Patch claude_desktop_config.json to register anno-rag MCP server.
///
/// Writes the anno-rag entry atomically, preserving existing mcpServers.
/// The binary path is the resolved path of the current executable (anno-rag-setup)
/// stripped to its directory, then `anno-rag` binary.
#[tauri::command]
pub async fn patch_claude_config() -> StepResult {
    match do_patch_claude_config() {
        Ok(path) => StepResult {
            ok: true,
            message: format!("Claude Desktop config updated: {}", path.display()),
        },
        Err(e) => StepResult {
            ok: false,
            message: format!("Failed to patch config: {e:#}"),
        },
    }
}

fn claude_config_path() -> Option<PathBuf> {
    // macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
    // Windows: %APPDATA%\Claude\claude_desktop_config.json
    dirs::config_dir().map(|p| p.join("Claude").join("claude_desktop_config.json"))
}

fn anno_rag_binary_path() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("current_exe")?;
    let dir = exe.parent().context("exe parent")?;
    let bin = if cfg!(target_os = "windows") {
        dir.join("anno-rag.exe")
    } else {
        dir.join("anno-rag")
    };
    anyhow::ensure!(bin.is_file(), "anno-rag binary not found at {}", bin.display());
    Ok(bin)
}

fn do_patch_claude_config() -> anyhow::Result<PathBuf> {
    let config_path = claude_config_path().context("cannot locate Claude config dir")?;
    let binary = anno_rag_binary_path()?;

    // Read existing config or start fresh.
    let mut config: serde_json::Value = if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path).context("read config")?;
        serde_json::from_str(&raw).context("parse claude_desktop_config.json — file may be malformed; fix or back it up before retrying")?
    } else {
        serde_json::json!({})
    };

    // Inject anno-rag entry under mcpServers.
    config
        .as_object_mut()
        .context("config is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("mcpServers is not an object")?
        .insert(
            "anno-rag".to_string(),
            serde_json::json!({
                "command": binary.to_string_lossy(),
                "args": ["mcp"]
            }),
        );

    // Atomic write: write to temp file then rename.
    let parent = config_path.parent().context("config parent")?;
    std::fs::create_dir_all(parent).context("create config dir")?;
    let tmp = config_path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&config)?).context("write tmp")?;
    // On Windows, rename fails if destination exists — remove first.
    if config_path.exists() {
        std::fs::remove_file(&config_path).context("remove old config before rename")?;
    }
    std::fs::rename(&tmp, &config_path).context("rename")?;

    Ok(config_path)
}

/// Approximate total size of all anno-rag models in MB.
/// Update this when the default model set changes.
const MODEL_TOTAL_MB: f32 = 545.0;

/// Download the three anno-rag models with progress events sent to the UI.
#[tauri::command]
pub async fn download_models_progress(on_progress: Channel<DownloadProgress>) -> StepResult {
    // Emit a fake-start so the UI immediately shows something.
    let _ = on_progress.send(DownloadProgress {
        pct: 0,
        current_file: "Connecting\u{2026}".to_string(),
        downloaded_mb: 0.0,
        total_mb: MODEL_TOTAL_MB,
    });

    let cfg = anno_rag::config::AnnoRagConfig::default();
    match anno_rag::download_models::download(&cfg).await {
        Ok(_) => {
            let _ = on_progress.send(DownloadProgress {
                pct: 100,
                current_file: "Done".to_string(),
                downloaded_mb: MODEL_TOTAL_MB,
                total_mb: MODEL_TOTAL_MB,
            });
            StepResult {
                ok: true,
                message: format!("Models ready (~{MODEL_TOTAL_MB:.0} MB)"),
            }
        }
        Err(e) => StepResult {
            ok: false,
            message: format!("Download failed: {e:#}"),
        },
    }
}

/// Initialise the vault keyring entry (generate key, store in OS keyring).
///
/// On Windows this uses DPAPI via the `keyring` crate.
/// On macOS this uses the Keychain.
#[tauri::command]
pub async fn init_vault_keyring() -> StepResult {
    match anno_rag::vault::init_keyring() {
        Ok(()) => StepResult {
            ok: true,
            message: "Vault key stored in OS keyring.".to_string(),
        },
        Err(e) => StepResult {
            ok: false,
            message: format!("Vault init failed: {e:#}"),
        },
    }
}
