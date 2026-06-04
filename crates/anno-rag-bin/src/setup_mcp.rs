use anno_rag::AnnoRagConfig;
use clap::{Args, ValueEnum};
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
