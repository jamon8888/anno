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
    #[arg(long = "allowed-root", value_name = "DIR")]
    pub allowed_roots: Vec<PathBuf>,
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
    /// Whether the write actually modified the target file. Retained for
    /// callers/telemetry even when the current binary path ignores it.
    #[allow(dead_code)]
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
        anyhow::bail!(
            "Claude Desktop config auto-detection is supported on Windows and macOS only; pass --desktop-config for managed clients"
        )
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

pub fn read_json_file_or_empty(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

const CREATE_NEW_ATTEMPTS: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TempCleanup {
    Remove,
    /// Keep the temp file for post-mortem on certain failures. Reserved
    /// API — not constructed on the current code path.
    #[allow(dead_code)]
    Preserve,
}

#[derive(Debug)]
struct ReplaceFileError {
    error: anyhow::Error,
    temp_cleanup: TempCleanup,
}

impl ReplaceFileError {
    fn remove_temp(error: anyhow::Error) -> Self {
        Self {
            error,
            temp_cleanup: TempCleanup::Remove,
        }
    }

    #[allow(dead_code)]
    fn preserve_temp(error: anyhow::Error) -> Self {
        Self {
            error,
            temp_cleanup: TempCleanup::Preserve,
        }
    }

    fn should_remove_temp(&self) -> bool {
        self.temp_cleanup == TempCleanup::Remove
    }

    fn into_error(self) -> anyhow::Error {
        self.error
    }
}

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
        if error.should_remove_temp() {
            remove_temp_file(&tmp);
        }
        return Err(error.into_error());
    }

    Ok(WriteResult {
        changed: true,
        message: format!("wrote {}", path.display()),
    })
}

pub fn build_claude_code_args(
    scope: ClaudeCodeScope,
    models_dir: &Path,
    binary: &Path,
    allowed_roots: &[PathBuf],
) -> anyhow::Result<Vec<String>> {
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    validate_absolute_path(binary).map_err(|e| anyhow!(e))?;

    let models_dir = path_to_config_string(models_dir)?;
    let binary = path_to_config_string(binary)?;

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
}

pub fn configure_claude_code(
    scope: ClaudeCodeScope,
    models_dir: &Path,
    binary: &Path,
    allowed_roots: &[PathBuf],
    dry_run: bool,
) -> anyhow::Result<String> {
    let args = build_claude_code_args(scope, models_dir, binary, allowed_roots)?;
    let command = display_command("claude", &args);
    if dry_run {
        return Ok(format!("dry-run: {command}"));
    }

    let output = std::process::Command::new("claude").args(&args).output();

    match output {
        Ok(out) if out.status.success() => {
            Ok("configured Claude Code MCP server anno-rag".to_string())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("claude mcp add failed: {}", stderr.trim());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(format!("Claude Code CLI not found; run later: {command}"))
        }
        Err(error) => Err(error).context("run claude mcp add"),
    }
}

fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|arg| display_command_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_command_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\''))
    {
        return arg.to_string();
    }

    format!("\"{}\"", arg.replace('"', "\\\""))
}

const GLINER_ONNX_BASES: &[&str] = &[
    "classifier",
    "count_lstm_fixed",
    "count_pred_argmax",
    "encoder",
    "schema_gather",
    "scorer",
    "span_rep",
    "token_gather",
];

pub fn model_cache_verified(models_dir: &Path, embedder_dir: &str, ner_onnx_dir: &str) -> bool {
    let embedder_files = anno_rag_mcp::model_inventory::embedder_required_files(embedder_dir);
    let embedder_refs: Vec<&str> = embedder_files.iter().map(String::as_str).collect();
    required_files_present(models_dir, &embedder_refs)
        && gliner_onnx_cache_verified(models_dir, ner_onnx_dir)
}

fn required_files_present(root: &Path, required_files: &[&str]) -> bool {
    required_files
        .iter()
        .all(|relative| root.join(relative).is_file())
}

fn gliner_onnx_cache_verified(models_dir: &Path, ner_onnx_dir: &str) -> bool {
    [("fp32_v2", "fp32"), ("fp16_v2", "fp16")]
        .iter()
        .any(|(variant_dir, suffix)| {
            let graph_files_ready = GLINER_ONNX_BASES.iter().all(|base| {
                models_dir
                    .join(ner_onnx_dir)
                    .join(variant_dir)
                    .join(format!("{base}_{suffix}.onnx"))
                    .is_file()
            });
            let tokenizer_ready = models_dir
                .join(ner_onnx_dir)
                .join(variant_dir)
                .join("tokenizer.json")
                .is_file()
                || models_dir
                    .join(ner_onnx_dir)
                    .join("tokenizer.json")
                    .is_file();
            graph_files_ready && tokenizer_ready
        })
}

pub async fn ensure_models(
    models_dir: &Path,
    skip_models: bool,
    dry_run: bool,
    cfg: &AnnoRagConfig,
) -> anyhow::Result<bool> {
    let embedder_dir = cfg.embedder_dir();
    let ner_onnx_dir = cfg.ner_onnx_dir();
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    if model_cache_verified(models_dir, &embedder_dir, &ner_onnx_dir) {
        return Ok(true);
    }
    if skip_models || dry_run {
        return Ok(false);
    }
    if models_dir.file_name().and_then(|name| name.to_str()) != Some("models") {
        anyhow::bail!(
            "--models-dir must end with 'models' for anno-rag download-models compatibility"
        );
    }

    let mut download_cfg = cfg.clone();
    download_cfg.data_dir = models_dir
        .parent()
        .map(Path::to_path_buf)
        .context("--models-dir must have a parent directory")?;
    anno_rag::download_models::download(&download_cfg).await?;
    Ok(model_cache_verified(
        models_dir,
        &embedder_dir,
        &ner_onnx_dir,
    ))
}

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

pub async fn run_fast_mcp_smoke(
    binary: &Path,
    models_dir: &Path,
    dry_run: bool,
) -> anyhow::Result<String> {
    validate_absolute_path(binary).map_err(|e| anyhow!(e))?;
    validate_absolute_path(models_dir).map_err(|e| anyhow!(e))?;
    if dry_run {
        return Ok(format!(
            "dry-run: would smoke-test {} mcp",
            binary.display()
        ));
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

    match run_fast_mcp_smoke_child(&mut child).await {
        Ok(()) => {
            let _ = child.kill().await;
            Ok("fast MCP smoke passed".to_string())
        }
        Err(error) => {
            let _ = child.kill().await;
            Err(error)
        }
    }
}

async fn run_fast_mcp_smoke_child(child: &mut tokio::process::Child) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut stdin = child.stdin.take().context("open mcp stdin")?;
    let stdout = child.stdout.take().context("open mcp stdout")?;
    let mut lines = BufReader::new(stdout).lines();

    stdin
        .write_all(
            jsonrpc_line(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "anno-setup", "version": "1.0"}
                }),
            )
            .as_bytes(),
        )
        .await
        .context("write MCP initialize")?;
    let init = read_mcp_line(&mut lines, "initialize").await?;
    let init_value: Value = serde_json::from_str(&init).context("parse MCP initialize response")?;
    if init_value["id"] != json!(1) {
        anyhow::bail!("MCP initialize failed: {init}");
    }

    stdin
        .write_all(notification_line("notifications/initialized").as_bytes())
        .await
        .context("write MCP initialized notification")?;
    stdin
        .write_all(jsonrpc_line(2, "tools/list", json!({})).as_bytes())
        .await
        .context("write MCP tools/list")?;
    let tools = read_mcp_line(&mut lines, "tools/list").await?;
    if !tools.contains("anno_health") {
        anyhow::bail!("MCP tools/list did not include anno_health: {tools}");
    }

    Ok(())
}

async fn read_mcp_line(
    lines: &mut tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    label: &str,
) -> anyhow::Result<String> {
    tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
        .await
        .with_context(|| format!("timeout waiting for MCP {label} response"))?
        .with_context(|| format!("read MCP {label} response"))?
        .with_context(|| format!("MCP {label} response stream ended"))
}

pub async fn run(args: SetupMcpArgs) -> anyhow::Result<()> {
    let binary = match args.binary {
        Some(path) => path,
        None => std::env::current_exe().context("resolve current executable")?,
    };
    validate_absolute_path(&binary).map_err(|e| anyhow!(e))?;

    let models_dir = args.models_dir.unwrap_or_else(default_models_dir);
    validate_absolute_path(&models_dir).map_err(|e| anyhow!(e))?;
    let cfg = AnnoRagConfig::load(None).unwrap_or_else(|e| {
        tracing::warn!("config load error: {e}; using defaults");
        AnnoRagConfig::default()
    });
    let models_verified = if args.target == SetupTarget::Manual {
        model_cache_verified(&models_dir, &cfg.embedder_dir(), &cfg.ner_onnx_dir())
    } else {
        ensure_models(&models_dir, args.skip_models, args.dry_run, &cfg).await?
    };

    let mut summary = Vec::<String>::new();
    summary.push(format!("binary: {}", binary.display()));
    summary.push(format!("models_dir: {}", models_dir.display()));
    summary.push(format!("models_verified: {models_verified}"));
    if let Some(roots) = allowed_roots_env_value(&args.allowed_roots)? {
        summary.push(format!("allowed_roots: {roots}"));
    } else {
        summary.push("allowed_roots: not configured".to_string());
    }

    if args.target == SetupTarget::Manual {
        let desktop = merge_desktop_config(
            json!({}),
            &binary,
            &models_dir,
            models_verified,
            &args.allowed_roots,
        )?;
        summary.push(format!(
            "desktop_json: {}",
            serde_json::to_string_pretty(&desktop)?
        ));
        summary.push(format!(
            "claude_code_command: {}",
            display_command(
                "claude",
                &build_claude_code_args(
                    args.claude_code_scope,
                    &models_dir,
                    &binary,
                    &args.allowed_roots,
                )?,
            )
        ));
        println!("{}", summary.join("\n"));
        return Ok(());
    }

    if args.target.includes_desktop() {
        if args.desktop_mode == DesktopMode::Mcpb {
            summary.push(
                "desktop: .mcpb install is interactive; use the release .mcpb asset or rerun with --desktop-mode json"
                    .to_string(),
            );
        } else {
            let config_path = match args.desktop_config {
                Some(path) => path,
                None => default_desktop_config_path()?,
            };
            let existing = read_json_file_or_empty(&config_path)?;
            let merged = merge_desktop_config(
                existing,
                &binary,
                &models_dir,
                models_verified,
                &args.allowed_roots,
            )?;
            let result = write_desktop_config(&config_path, &merged, args.dry_run)?;
            summary.push(format!("desktop: {}", result.message));
        }
    }

    if args.target.includes_claude_code() {
        let result = configure_claude_code(
            args.claude_code_scope,
            &models_dir,
            &binary,
            &args.allowed_roots,
            args.dry_run,
        )?;
        summary.push(format!("claude_code: {result}"));
    }

    if !args.dry_run {
        match run_fast_mcp_smoke(&binary, &models_dir, false).await {
            Ok(msg) => summary.push(format!("smoke: {msg}")),
            Err(error) => summary.push(format!("smoke_warning: {error}")),
        }
    }

    summary.push("restart Claude Desktop/Cowork before verifying anno_health".to_string());
    println!("{}", summary.join("\n"));
    Ok(())
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
fn replace_file(tmp: &Path, path: &Path) -> Result<(), ReplaceFileError> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "ReplaceFileW"]
        fn replace_file_w(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut core::ffi::c_void,
            reserved: *mut core::ffi::c_void,
        ) -> i32;
    }

    if !path.exists() {
        if let Err(error) = std::fs::rename(tmp, path)
            .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))
        {
            return Err(ReplaceFileError::remove_temp(error));
        }
        return Ok(());
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
        replace_file_w(
            path_wide.as_ptr(),
            tmp_wide.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        let error = anyhow!(std::io::Error::last_os_error()).context(format!(
            "replace {} with {}; preserved replacement temp at {} for recovery",
            path.display(),
            tmp.display(),
            tmp.display()
        ));
        return Err(ReplaceFileError::preserve_temp(error));
    }

    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, path: &Path) -> Result<(), ReplaceFileError> {
    if let Err(error) = std::fs::rename(tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))
    {
        return Err(ReplaceFileError::remove_temp(error));
    }
    Ok(())
}

pub fn merge_desktop_config(
    mut existing: Value,
    binary: &Path,
    models_dir: &Path,
    models_verified: bool,
    allowed_roots: &[PathBuf],
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
    if let Some(roots) = allowed_roots_env_value(allowed_roots)? {
        env.insert("ANNO_RAG_ALLOWED_ROOTS".to_string(), Value::String(roots));
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
    fn replace_file_error_cleanup_policy_can_preserve_temp() {
        let removable = ReplaceFileError::remove_temp(anyhow!("remove"));
        let preserved = ReplaceFileError::preserve_temp(anyhow!("preserve"));

        assert!(removable.should_remove_temp());
        assert!(!preserved.should_remove_temp());
    }

    #[test]
    fn claude_code_args_include_scope_env_binary_and_mcp() {
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let expected_models_dir = path_to_config_string(&models_dir).expect("models path");
        let expected_binary = path_to_config_string(&binary).expect("binary path");

        let args =
            build_claude_code_args(ClaudeCodeScope::User, &models_dir, &binary, &[]).expect("args");

        assert_eq!(args[0], "mcp");
        assert_eq!(args[1], "add");
        assert_eq!(
            args,
            vec![
                "mcp".to_string(),
                "add".to_string(),
                "--transport".to_string(),
                "stdio".to_string(),
                "--scope".to_string(),
                "user".to_string(),
                "--env".to_string(),
                format!("ANNO_MODELS_DIR={expected_models_dir}"),
                "anno-rag".to_string(),
                "--".to_string(),
                expected_binary,
                "mcp".to_string(),
            ]
        );
    }

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

        let expected = format!(
            "ANNO_RAG_ALLOWED_ROOTS={};{}",
            path_to_config_string(&root_a).expect("root a"),
            path_to_config_string(&root_b).expect("root b")
        );
        assert!(args.contains(&"--env".to_string()));
        assert!(args.contains(&expected));
    }

    #[test]
    fn claude_code_args_reject_relative_paths() {
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let err = build_claude_code_args(
            ClaudeCodeScope::User,
            &models_dir,
            Path::new("anno-rag"),
            &[],
        )
        .expect_err("relative binary must fail");

        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn claude_code_args_reject_relative_models_dir() {
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let err = build_claude_code_args(ClaudeCodeScope::User, Path::new("models"), &binary, &[])
            .expect_err("relative models dir must fail");

        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn configure_claude_code_dry_run_returns_command_without_running_cli() {
        let models_dir = absolute_test_path(&["anno rag", "models"]);
        let binary = absolute_test_path(&["hacienda tools", "anno-rag"]);
        let result =
            configure_claude_code(ClaudeCodeScope::Project, &models_dir, &binary, &[], true)
                .expect("dry run");

        assert!(result.starts_with("dry-run: claude mcp add"));
        assert!(result.contains("--scope project"));
        assert!(result.contains("--env"));
        assert!(result.contains("\"ANNO_MODELS_DIR="));
        assert!(result.ends_with(" mcp"));
    }

    #[test]
    fn initialize_payload_is_jsonrpc_line() {
        let line = jsonrpc_line(
            1,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "anno-setup", "version": "1.0"}
            }),
        );

        assert!(line.ends_with('\n'));
        let parsed: Value = serde_json::from_str(line.trim()).expect("json");
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "initialize");
    }

    #[tokio::test]
    async fn fast_mcp_smoke_dry_run_does_not_spawn() {
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);

        let result = run_fast_mcp_smoke(&binary, &models_dir, true)
            .await
            .expect("dry run");

        assert!(result.starts_with("dry-run: would smoke-test"));
        assert!(result.ends_with(" mcp"));
    }

    #[tokio::test]
    async fn run_manual_does_not_download_or_require_models_suffix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models_dir = dir.path().join("manual-cache");
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let args = SetupMcpArgs {
            target: SetupTarget::Manual,
            binary: Some(binary),
            models_dir: Some(models_dir.clone()),
            allowed_roots: Vec::new(),
            desktop_config: None,
            desktop_mode: DesktopMode::Json,
            claude_code_scope: ClaudeCodeScope::User,
            skip_models: false,
            dry_run: false,
            force: false,
        };

        run(args).await.expect("manual run");

        assert!(!models_dir.exists());
    }

    fn write_required_files(root: &Path, files: &[&str]) {
        for relative in files {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create parent");
            }
            std::fs::write(path, "x").expect("write model file");
        }
    }

    fn write_fp16_gliner_files(root: &Path) {
        let gliner = root.join("gliner2-multi-v1-onnx");
        std::fs::create_dir_all(gliner.join("fp16_v2")).expect("create fp16");
        for base in GLINER_ONNX_BASES {
            std::fs::write(
                gliner.join("fp16_v2").join(format!("{base}_fp16.onnx")),
                "x",
            )
            .expect("write fp16 graph");
        }
        std::fs::write(gliner.join("tokenizer.json"), "{}").expect("write tokenizer");
    }

    #[test]
    fn model_cache_verified_when_expected_families_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        let embedder_files =
            anno_rag_mcp::model_inventory::embedder_required_files("Solon-embeddings-large-0.1");
        let embedder_refs: Vec<&str> = embedder_files.iter().map(String::as_str).collect();
        write_required_files(&models, &embedder_refs);
        #[allow(deprecated)]
        write_required_files(
            &models,
            anno_rag_mcp::model_inventory::GLINER_REQUIRED_FILES,
        );

        assert!(model_cache_verified(
            &models,
            "Solon-embeddings-large-0.1",
            "gliner2-multi-v1-onnx"
        ));
    }

    #[test]
    fn model_cache_verified_with_fp16_gliner_variant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        let embedder_files =
            anno_rag_mcp::model_inventory::embedder_required_files("Solon-embeddings-large-0.1");
        let embedder_refs: Vec<&str> = embedder_files.iter().map(String::as_str).collect();
        write_required_files(&models, &embedder_refs);
        write_fp16_gliner_files(&models);

        assert!(model_cache_verified(
            &models,
            "Solon-embeddings-large-0.1",
            "gliner2-multi-v1-onnx"
        ));
    }

    #[test]
    fn model_cache_not_verified_when_gliner_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        let embedder_files =
            anno_rag_mcp::model_inventory::embedder_required_files("Solon-embeddings-large-0.1");
        let embedder_refs: Vec<&str> = embedder_files.iter().map(String::as_str).collect();
        write_required_files(&models, &embedder_refs);

        assert!(!model_cache_verified(
            &models,
            "Solon-embeddings-large-0.1",
            "gliner2-multi-v1-onnx"
        ));
    }

    #[test]
    fn model_cache_not_verified_when_embedder_weights_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        // Write config.json and tokenizer.json but NOT model.safetensors — incomplete embedder.
        std::fs::create_dir_all(models.join("Solon-embeddings-large-0.1")).expect("embedder dir");
        std::fs::write(
            models
                .join("Solon-embeddings-large-0.1")
                .join("config.json"),
            "{}",
        )
        .expect("embedder config");
        std::fs::write(
            models
                .join("Solon-embeddings-large-0.1")
                .join("tokenizer.json"),
            "{}",
        )
        .expect("embedder tokenizer");
        #[allow(deprecated)]
        write_required_files(
            &models,
            anno_rag_mcp::model_inventory::GLINER_REQUIRED_FILES,
        );

        assert!(!model_cache_verified(
            &models,
            "Solon-embeddings-large-0.1",
            "gliner2-multi-v1-onnx"
        ));
    }

    #[tokio::test]
    async fn ensure_models_returns_true_when_cache_verified() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");
        let embedder_files =
            anno_rag_mcp::model_inventory::embedder_required_files("Solon-embeddings-large-0.1");
        let embedder_refs: Vec<&str> = embedder_files.iter().map(String::as_str).collect();
        write_required_files(&models, &embedder_refs);
        #[allow(deprecated)]
        write_required_files(
            &models,
            anno_rag_mcp::model_inventory::GLINER_REQUIRED_FILES,
        );

        assert!(
            ensure_models(&models, false, false, &AnnoRagConfig::default())
                .await
                .expect("ensure")
        );
    }

    #[tokio::test]
    async fn ensure_models_returns_false_on_dry_run_without_download() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");

        assert!(
            !ensure_models(&models, false, true, &AnnoRagConfig::default())
                .await
                .expect("ensure")
        );
    }

    #[tokio::test]
    async fn ensure_models_returns_false_when_skipping_missing_models() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("models");

        assert!(
            !ensure_models(&models, true, false, &AnnoRagConfig::default())
                .await
                .expect("ensure")
        );
    }

    #[tokio::test]
    async fn ensure_models_rejects_non_models_dir_when_download_required() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = dir.path().join("model-cache");
        let err = ensure_models(&models, false, false, &AnnoRagConfig::default())
            .await
            .expect_err("non-models dir must fail before download");

        assert!(err.to_string().contains("models"));
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
        let merged =
            merge_desktop_config(existing, &binary, &models_dir, true, &[]).expect("merge");

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
    fn desktop_config_includes_allowed_roots_env_when_configured() {
        let binary = absolute_test_path(&["hacienda", "anno-rag.exe"]);
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let root = absolute_test_path(&["clients"]);

        let merged = merge_desktop_config(json!({}), &binary, &models_dir, true, &[root.clone()])
            .expect("merge");
        let env = merged["mcpServers"]["anno-rag"]["env"]
            .as_object()
            .expect("anno-rag env");

        assert_eq!(
            env.get("ANNO_RAG_ALLOWED_ROOTS")
                .and_then(|value| value.as_str()),
            Some(path_to_config_string(&root).expect("root").as_str())
        );
    }

    #[test]
    fn desktop_merge_omits_no_downloads_when_models_are_not_verified() {
        let binary = absolute_test_path(&["hacienda", "anno-rag"]);
        let models_dir = absolute_test_path(&["anno-rag", "models"]);
        let merged =
            merge_desktop_config(json!({}), &binary, &models_dir, false, &[]).expect("merge");

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
        let err = merge_desktop_config(
            json!(["not", "an", "object"]),
            &binary,
            &models_dir,
            true,
            &[],
        )
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

    #[cfg(windows)]
    #[test]
    fn windows_replace_file_updates_target_and_removes_temp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        let tmp_path = desktop_config_temp_path(&config_path, "replace-test", 0);
        std::fs::write(&config_path, "old").expect("seed");
        std::fs::write(&tmp_path, "new").expect("tmp");

        replace_file(&tmp_path, &config_path).expect("replace");

        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read config"),
            "new"
        );
        assert!(!tmp_path.exists());
    }

    #[cfg(windows)]
    fn windows_path(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;

        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    #[cfg(windows)]
    fn get_windows_file_attributes(path: &Path) -> u32 {
        const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            #[link_name = "GetFileAttributesW"]
            fn get_file_attributes_w(file_name: *const u16) -> u32;
        }

        let path = windows_path(path);
        // SAFETY: path is a null-terminated UTF-16 buffer that lives for this call.
        let attributes = unsafe { get_file_attributes_w(path.as_ptr()) };
        assert_ne!(
            attributes, INVALID_FILE_ATTRIBUTES,
            "GetFileAttributesW failed"
        );
        attributes
    }

    #[cfg(windows)]
    fn set_windows_file_attributes(path: &Path, attributes: u32) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            #[link_name = "SetFileAttributesW"]
            fn set_file_attributes_w(file_name: *const u16, file_attributes: u32) -> i32;
        }

        let path = windows_path(path);
        // SAFETY: path is a null-terminated UTF-16 buffer that lives for this call.
        let updated = unsafe { set_file_attributes_w(path.as_ptr(), attributes) };
        assert_ne!(updated, 0, "SetFileAttributesW failed");
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_file_preserves_existing_target_attributes() {
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("claude_desktop_config.json");
        let tmp_path = desktop_config_temp_path(&config_path, "attribute-test", 0);
        std::fs::write(&config_path, "old").expect("seed");
        std::fs::write(&tmp_path, "new").expect("tmp");

        let original_attributes = get_windows_file_attributes(&config_path);
        set_windows_file_attributes(&config_path, original_attributes | FILE_ATTRIBUTE_HIDDEN);

        replace_file(&tmp_path, &config_path).expect("replace");

        let replaced_attributes = get_windows_file_attributes(&config_path);
        assert_ne!(
            replaced_attributes & FILE_ATTRIBUTE_HIDDEN,
            0,
            "ReplaceFileW should preserve attributes from the existing target"
        );
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read config"),
            "new"
        );
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
