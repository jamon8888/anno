use anno_rag::AnnoRagConfig;
use anyhow::{anyhow, Context};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};
use std::io::Write;
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

#[derive(Debug, Clone)]
pub struct WriteResult {
    pub changed: bool,
    pub message: String,
}

pub fn default_models_dir() -> PathBuf {
    AnnoRagConfig::default().models_cache()
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

pub fn read_json_file_or_empty(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

const CREATE_NEW_ATTEMPTS: u32 = 100;

pub fn write_desktop_config(
    path: &Path,
    value: &Value,
    dry_run: bool,
) -> anyhow::Result<WriteResult> {
    if dry_run {
        return Ok(WriteResult {
            changed: false,
            message: format!("dry-run: would write {}", path.display()),
        });
    }

    create_parent_dir(path)?;
    let write_id = desktop_config_write_id();

    let existing_permissions = if path.exists() {
        let metadata =
            std::fs::metadata(path).with_context(|| format!("metadata {}", path.display()))?;
        copy_desktop_config_backup(path, &write_id)?;
        Some(metadata.permissions())
    } else {
        None
    };

    let (tmp, tmp_file) = create_unique_temp_file(path, &write_id)?;
    if let Err(error) = write_temp_config(&tmp, tmp_file, value, existing_permissions) {
        remove_temp_file(&tmp);
        return Err(error);
    }

    if let Err(error) = replace_file(&tmp, path) {
        remove_temp_file(&tmp);
        return Err(error);
    }

    Ok(WriteResult {
        changed: true,
        message: format!("wrote {}", path.display()),
    })
}

fn create_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    Ok(())
}

fn desktop_config_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "claude_desktop_config.json".to_string())
}

fn desktop_config_write_id() -> String {
    format!(
        "{}.{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S%.9f"),
        uuid::Uuid::now_v7()
    )
}

fn desktop_config_backup_path(path: &Path, write_id: &str, attempt: u32) -> PathBuf {
    path.with_file_name(format!(
        "{}.bak.{}.{}",
        desktop_config_file_name(path),
        write_id,
        attempt
    ))
}

fn desktop_config_temp_path(path: &Path, write_id: &str, attempt: u32) -> PathBuf {
    path.with_file_name(format!(
        ".{}.tmp.{}.{}",
        desktop_config_file_name(path),
        write_id,
        attempt
    ))
}

fn create_unique_file(
    path: &Path,
    write_id: &str,
    label: &str,
    path_for_attempt: fn(&Path, &str, u32) -> PathBuf,
) -> anyhow::Result<(PathBuf, std::fs::File)> {
    for attempt in 0..CREATE_NEW_ATTEMPTS {
        let candidate = path_for_attempt(path, write_id, attempt);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("create {}", candidate.display()));
            }
        }
    }

    anyhow::bail!(
        "unable to allocate unique {} path for {}",
        label,
        path.display()
    )
}

fn create_unique_temp_file(
    path: &Path,
    write_id: &str,
) -> anyhow::Result<(PathBuf, std::fs::File)> {
    create_unique_file(path, write_id, "temp", desktop_config_temp_path)
}

fn copy_desktop_config_backup(path: &Path, write_id: &str) -> anyhow::Result<PathBuf> {
    let mut source =
        std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let (backup, mut backup_file) =
        create_unique_file(path, write_id, "backup", desktop_config_backup_path)?;

    let copy_result = (|| -> anyhow::Result<()> {
        std::io::copy(&mut source, &mut backup_file)
            .with_context(|| format!("backup {} to {}", path.display(), backup.display()))?;
        backup_file
            .sync_all()
            .with_context(|| format!("sync {}", backup.display()))?;
        Ok(())
    })();

    if let Err(error) = copy_result {
        remove_temp_file(&backup);
        return Err(error);
    }

    Ok(backup)
}

fn write_temp_config(
    tmp: &Path,
    mut tmp_file: std::fs::File,
    value: &Value,
    existing_permissions: Option<std::fs::Permissions>,
) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut tmp_file, value).context("serialize desktop config")?;
    tmp_file
        .write_all(b"\n")
        .with_context(|| format!("write {}", tmp.display()))?;

    if let Some(permissions) = existing_permissions {
        std::fs::set_permissions(tmp, permissions)
            .with_context(|| format!("set permissions {}", tmp.display()))?;
    }

    tmp_file
        .sync_all()
        .with_context(|| format!("sync {}", tmp.display()))?;
    Ok(())
}

fn remove_temp_file(path: &Path) {
    make_temp_writable_for_cleanup(path);
    let _ = std::fs::remove_file(path);
}

#[cfg(windows)]
fn make_temp_writable_for_cleanup(path: &Path) {
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

#[cfg(not(windows))]
fn make_temp_writable_for_cleanup(_path: &Path) {}

#[cfg(windows)]
fn replace_file(tmp: &Path, path: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "MoveFileExW"]
        fn move_file_ex_w(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let tmp_wide: Vec<u16> = tmp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let path_wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: both buffers are null-terminated UTF-16 paths and live for the duration of the call.
    let replaced = unsafe {
        move_file_ex_w(
            tmp_wide.as_ptr(),
            path_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("rename {} to {}", tmp.display(), path.display()));
    }

    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, path: &Path) -> anyhow::Result<()> {
    std::fs::rename(tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
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

    #[test]
    fn dry_run_does_not_write_desktop_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        let merged =
            json!({"mcpServers":{"anno-rag":{"command":"C:/Tools/anno-rag.exe","args":["mcp"]}}});

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

        let merged =
            json!({"mcpServers":{"anno-rag":{"command":"C:/Tools/anno-rag.exe","args":["mcp"]}}});
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
    fn repeated_writes_create_two_distinct_backups() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&config_path, "{\"mcpServers\":{}}").expect("seed");

        let first =
            json!({"mcpServers":{"anno-rag":{"command":"C:/Tools/anno-rag.exe","args":["mcp"]}}});
        let second = json!({"mcpServers":{"anno-rag":{"command":"C:/Tools/anno-rag-v2.exe","args":["mcp"]}}});

        write_desktop_config(&config_path, &first, false).expect("first write");
        write_desktop_config(&config_path, &second, false).expect("second write");

        let mut backups: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".bak."))
            .collect();
        backups.sort();

        assert_eq!(backups.len(), 2);
        assert_ne!(backups[0], backups[1]);
    }

    #[test]
    fn temp_paths_stay_in_target_parent_and_vary_by_attempt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");

        let first = desktop_config_temp_path(&config_path, "write-id", 0);
        let second = desktop_config_temp_path(&config_path, "write-id", 1);

        assert_eq!(first.parent(), config_path.parent());
        assert_eq!(second.parent(), config_path.parent());
        assert_ne!(first, second);
    }

    #[cfg(unix)]
    #[test]
    fn write_desktop_config_preserves_existing_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&config_path, "{\"mcpServers\":{}}").expect("seed");
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))
            .expect("set mode");

        let merged =
            json!({"mcpServers":{"anno-rag":{"command":"/usr/local/bin/anno-rag","args":["mcp"]}}});
        write_desktop_config(&config_path, &merged, false).expect("write");

        let mode = std::fs::metadata(&config_path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
