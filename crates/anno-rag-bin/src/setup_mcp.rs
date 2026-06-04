use anno_rag::AnnoRagConfig;
use anyhow::{anyhow, Context};
use clap::{Args, ValueEnum};
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

fn path_to_config_string(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .context("path must be valid UTF-8 for desktop config")
}

pub fn merge_desktop_config(
    mut existing: Value,
    binary: &Path,
    models_dir: &Path,
    models_verified: bool,
) -> anyhow::Result<Value> {
    validate_absolute_path(binary).map_err(|e| anyhow!(e))?;
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    let binary = path_to_config_string(binary)?;
    let models_dir = path_to_config_string(models_dir)?;

    let root = existing
        .as_object_mut()
        .context("desktop config root must be a JSON object")?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("mcpServers must be a JSON object")?;

    let mut env = serde_json::Map::new();
    env.insert("ANNO_MODELS_DIR".to_string(), Value::String(models_dir));
    if models_verified {
        env.insert(
            "ANNO_NO_DOWNLOADS".to_string(),
            Value::String("1".to_string()),
        );
    }

    servers.insert(
        "anno-rag".to_string(),
        json!({
            "command": binary,
            "args": ["mcp"],
            "env": env
        }),
    );

    Ok(existing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute_test_path(parts: &[&str]) -> PathBuf {
        let mut path = std::env::temp_dir();
        for part in parts {
            path.push(part);
        }
        path
    }

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

    #[test]
    fn desktop_merge_preserves_existing_servers_and_omits_vault_secret() {
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let filesystem_server = json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem"],
            "env": {
                "ROOT": "existing-root"
            }
        });
        let existing = json!({
            "theme": "dark",
            "mcpServers": {
                "filesystem": filesystem_server.clone(),
                "anno-rag": {
                    "command": "old-binary",
                    "args": ["old"],
                    "env": {
                        "ANNO_MODELS_DIR": "old-models",
                        "ANNO_RAG_VAULT_PASSPHRASE": "stale-secret"
                    }
                }
            }
        });
        let merged = merge_desktop_config(existing, &binary, &models_dir, true).expect("merge");

        assert_eq!(merged["theme"], "dark");
        assert_eq!(merged["mcpServers"]["filesystem"], filesystem_server);
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["command"],
            binary.to_str().expect("binary path")
        );
        assert_eq!(merged["mcpServers"]["anno-rag"]["args"], json!(["mcp"]));
        let anno_env = merged["mcpServers"]["anno-rag"]["env"]
            .as_object()
            .expect("anno-rag env");
        assert_eq!(
            anno_env
                .get("ANNO_MODELS_DIR")
                .and_then(|value| value.as_str()),
            Some(models_dir.to_str().expect("models path"))
        );
        assert_eq!(
            anno_env
                .get("ANNO_NO_DOWNLOADS")
                .and_then(|value| value.as_str()),
            Some("1")
        );
        assert!(!anno_env.contains_key("ANNO_RAG_VAULT_PASSPHRASE"));
    }

    #[test]
    fn desktop_merge_omits_no_downloads_when_models_are_not_verified() {
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let merged = merge_desktop_config(json!({}), &binary, &models_dir, false).expect("merge");

        assert!(merged["mcpServers"]["anno-rag"].is_object());
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["command"],
            binary.to_str().expect("binary path")
        );
        assert_eq!(merged["mcpServers"]["anno-rag"]["args"], json!(["mcp"]));
        assert_eq!(
            merged["mcpServers"]["anno-rag"]["env"]["ANNO_MODELS_DIR"],
            models_dir.to_str().expect("models path")
        );
        let anno_env = merged["mcpServers"]["anno-rag"]["env"]
            .as_object()
            .expect("anno-rag env");
        assert!(!anno_env.contains_key("ANNO_NO_DOWNLOADS"));
    }

    #[test]
    fn desktop_merge_rejects_non_object_root() {
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let err = merge_desktop_config(json!(["not", "an", "object"]), &binary, &models_dir, true)
            .expect_err("non-object roots must be rejected");

        assert!(err.to_string().contains("root"));
    }
}
