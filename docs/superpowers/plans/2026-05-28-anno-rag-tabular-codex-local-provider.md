# Anno RAG Tabular Codex Local Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a first local-subscription LLM provider for `anno-rag-tabular` by running the user-authenticated Codex CLI in non-interactive mode, without forwarding API keys.

**Architecture:** Keep the existing `LlmClient` trait unchanged. Add a shared local CLI helper module for temp files, environment sanitization, timeouts, output parsing, and PII preflight, then add `CodexLocalLlm` as a concrete provider using `codex exec --ephemeral --output-schema`. This plan implements only the Codex exec fallback slice; Codex app-server JSON-RPC, Claude local mode, and richer insight storage get separate plans.

**Tech Stack:** Rust 1.95, Tokio process/fs/io, serde_json, sha2, regex, existing `anno-rag-tabular::llm::LlmClient`, existing `scripts/dev-fast.ps1` local check loop.

---

## Scope

Implement Phase 1 from `docs/superpowers/specs/2026-05-28-anno-rag-tabular-local-subscription-llm-design.md`:

- local CLI infrastructure;
- `CodexLocalLlm` using `codex exec`;
- no API key forwarding in subscription mode;
- output JSON parsing through `StructuredOutput`;
- fake-binary tests;
- one ignored live smoke test.

Do not implement in this plan:

- Codex app-server JSON-RPC;
- Claude local provider;
- insight storage;
- UI settings;
- provider usage accounting beyond zero/default `Usage`.

## File Structure

- Modify `crates/anno-rag-tabular/src/llm/mod.rs`
  - Export the new modules.
  - Add provider config types and explicit `from_config`.
  - Keep `default_from_env()` backward compatible.

- Create `crates/anno-rag-tabular/src/llm/local_cli.rs`
  - Shared subprocess utility code.
  - Sanitized environment builder.
  - Obvious PII guard.
  - Temp workdir and schema/output file handling.
  - Timeout and exit-status handling.
  - JSON output loading.

- Create `crates/anno-rag-tabular/src/llm/codex_local.rs`
  - `CodexLocalLlm`.
  - `CodexLocalConfig`.
  - Command construction for `codex exec`.
  - `LlmClient` implementation.

- Create `crates/anno-rag-tabular/tests/codex_local_fake.rs`
  - Integration tests with fake `codex` executable.
  - Verifies output parsing and integration with `Extractor`.

- Create `crates/anno-rag-tabular/tests/codex_local_live.rs`
  - Ignored smoke test for a real installed and logged-in Codex CLI.

- No Cargo dependency change is required.
  - `tokio` already has `full` features in the workspace.
  - `serde_json`, `sha2`, and `regex` are already dependencies.
  - `tempfile` is already a dev-dependency for tests.

## Preflight

- [ ] **Step 1: Confirm GitNexus and worktree state**

Run:

```powershell
npx gitnexus status
git status --short
```

Expected:

```text
Status: ✅ up-to-date
```

If `gitnexus status` reports stale, run:

```powershell
npx gitnexus analyze
```

Do not stage or modify the existing unrelated files unless this task needs them. At the time this plan was written, unrelated changes existed in:

```text
.config/hakari.toml
crates/anno-rag-tabular/src/schema/column.rs
workspace-hack/Cargo.toml
docs/superpowers/plans/2026-05-27-anno-tabular-local-legal-extraction-quality.md
docs/superpowers/specs/2026-05-27-anno-tabular-local-legal-extraction-quality-design.md
```

- [ ] **Step 2: Run impact checks before editing symbols**

Run impact checks for the symbols that will be edited or extended:

```powershell
npx gitnexus impact LlmClient
npx gitnexus impact default_from_env
npx gitnexus impact StructuredOutput
```

Expected: no HIGH or CRITICAL risk for adding provider implementations behind the existing trait. If any impact check reports HIGH or CRITICAL, stop and summarize the blast radius before editing.

## Task 1: Add Local CLI Environment and PII Guard

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/local_cli.rs`
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`

- [ ] **Step 1: Write the failing unit tests for environment sanitization and PII guard**

Add this test module to the new file `crates/anno-rag-tabular/src/llm/local_cli.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn subscription_env_strips_api_key_variables() {
        let input = vec![
            (OsString::from("PATH"), OsString::from("bin")),
            (OsString::from("OPENAI_API_KEY"), OsString::from("sk-test")),
            (OsString::from("CODEX_API_KEY"), OsString::from("codex-test")),
            (OsString::from("ANTHROPIC_API_KEY"), OsString::from("anthropic-test")),
            (OsString::from("ANTHROPIC_AUTH_TOKEN"), OsString::from("oauth-test")),
            (OsString::from("ANTHROPIC_BASE_URL"), OsString::from("http://example.invalid")),
            (OsString::from("HOME"), OsString::from("C:/Users/test")),
        ];

        let sanitized = sanitized_subscription_env(input);
        let keys: Vec<String> = sanitized
            .iter()
            .map(|(key, _)| key.to_string_lossy().to_string())
            .collect();

        assert!(keys.contains(&"PATH".to_string()));
        assert!(keys.contains(&"HOME".to_string()));
        assert!(!keys.contains(&"OPENAI_API_KEY".to_string()));
        assert!(!keys.contains(&"CODEX_API_KEY".to_string()));
        assert!(!keys.contains(&"ANTHROPIC_API_KEY".to_string()));
        assert!(!keys.contains(&"ANTHROPIC_AUTH_TOKEN".to_string()));
        assert!(!keys.contains(&"ANTHROPIC_BASE_URL".to_string()));
    }

    #[test]
    fn obvious_pii_guard_accepts_pseudonymized_prompt() {
        let prompt = "[CHUNK::00000000-0000-0000-0000-000000000000]PERSON_1 represente ORG_1.[/CHUNK]";
        assert_eq!(find_obvious_clear_pii(prompt), None);
    }

    #[test]
    fn obvious_pii_guard_rejects_clear_email() {
        let prompt = "Contact: claire.fontaine@example.com";
        assert_eq!(find_obvious_clear_pii(prompt), Some("email"));
    }

    #[test]
    fn obvious_pii_guard_rejects_french_iban() {
        let prompt = "IBAN: FR76 3000 4000 0500 0612 3456 789";
        assert_eq!(find_obvious_clear_pii(prompt), Some("iban_fr"));
    }
}
```

- [ ] **Step 2: Export the new module and run the tests to verify failure**

Modify `crates/anno-rag-tabular/src/llm/mod.rs`:

```rust
pub mod anthropic;
pub(crate) mod local_cli;
pub mod mock;
```

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: FAIL because `sanitized_subscription_env` and `find_obvious_clear_pii` do not exist yet.

- [ ] **Step 3: Implement environment sanitization and the obvious PII guard**

Add this implementation above the test module in `crates/anno-rag-tabular/src/llm/local_cli.rs`:

```rust
use regex::Regex;
use std::ffi::OsString;

const SUBSCRIPTION_ENV_DENYLIST: &[&str] = &[
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
];

pub(crate) fn sanitized_subscription_env<I>(base: I) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    base.into_iter()
        .filter(|(key, _)| {
            let key = key.to_string_lossy();
            !SUBSCRIPTION_ENV_DENYLIST
                .iter()
                .any(|blocked| key.eq_ignore_ascii_case(blocked))
        })
        .collect()
}

pub(crate) fn find_obvious_clear_pii(prompt: &str) -> Option<&'static str> {
    let email = Regex::new(r"(?i)\b[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}\b")
        .expect("email regex compiles");
    if email.is_match(prompt) {
        return Some("email");
    }

    let iban_fr = Regex::new(r"\bFR\d{2}(?:[ ]?[0-9A-Z]{4}){5}[ ]?[0-9A-Z]{3}\b")
        .expect("iban regex compiles");
    if iban_fr.is_match(prompt) {
        return Some("iban_fr");
    }

    None
}
```

- [ ] **Step 4: Run the local CLI tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: PASS for all four tests in `llm::local_cli`.

- [ ] **Step 5: Run a targeted package check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
```

Expected: PASS.

- [ ] **Step 6: Commit Task 1**

Stage only the files touched in this task:

```powershell
git add -- crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/src/llm/local_cli.rs
git commit -m "feat(tabular): add local cli provider guards"
```

## Task 2: Add Local CLI Runner

**Files:**
- Modify: `crates/anno-rag-tabular/src/llm/local_cli.rs`

- [ ] **Step 1: Write failing tests for prompt size, JSON loading, and missing output**

Add these tests to `crates/anno-rag-tabular/src/llm/local_cli.rs` inside the existing test module:

```rust
#[tokio::test]
async fn request_rejects_prompt_over_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = write_request_files(
        dir.path(),
        "system",
        "abcdef",
        &serde_json::json!({"type": "object"}),
        5,
    )
    .await
    .expect_err("prompt over limit should fail");

    assert!(format!("{err}").contains("prompt is too large"));
}

#[tokio::test]
async fn write_request_files_writes_schema_prompt_and_output_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = write_request_files(
        dir.path(),
        "system prompt",
        "user prompt",
        &serde_json::json!({"type": "object"}),
        1024,
    )
    .await
    .expect("files written");

    let prompt = tokio::fs::read_to_string(&files.prompt_path)
        .await
        .expect("prompt file readable");
    let schema = tokio::fs::read_to_string(&files.schema_path)
        .await
        .expect("schema file readable");

    assert!(prompt.contains("system prompt"));
    assert!(prompt.contains("user prompt"));
    assert!(schema.contains("\"type\":\"object\""));
    assert_eq!(files.output_path.file_name().and_then(|s| s.to_str()), Some("result.json"));
}

#[tokio::test]
async fn read_output_json_rejects_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing.json");
    let err = read_output_json(&path)
        .await
        .expect_err("missing output should fail");

    assert!(format!("{err}").contains("provider did not write output JSON"));
}

#[tokio::test]
async fn read_output_json_parses_json_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("result.json");
    tokio::fs::write(&path, r#"{"ok":true}"#)
        .await
        .expect("write result");

    let value = read_output_json(&path).await.expect("json parsed");
    assert_eq!(value, serde_json::json!({"ok": true}));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: FAIL because `write_request_files`, `read_output_json`, and supporting types are not implemented.

- [ ] **Step 3: Implement request files and JSON output parsing**

Add these imports to `crates/anno-rag-tabular/src/llm/local_cli.rs`:

```rust
use crate::error::{Error, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
```

Add this code below `find_obvious_clear_pii`:

```rust
#[derive(Debug)]
pub(crate) struct LocalCliFiles {
    pub prompt_path: PathBuf,
    pub schema_path: PathBuf,
    pub output_path: PathBuf,
    pub prompt_hash: String,
    pub schema_hash: String,
}

#[derive(Debug)]
pub(crate) struct LocalCliError(String);

impl fmt::Display for LocalCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for LocalCliError {}

pub(crate) fn local_cli_extract_error(message: impl Into<String>) -> Error {
    Error::Extract {
        doc: "local-cli".into(),
        col: "*".into(),
        source: Box::new(LocalCliError(message.into())),
    }
}

pub(crate) async fn write_request_files(
    dir: &Path,
    system: &str,
    user: &str,
    json_schema: &Value,
    max_prompt_bytes: usize,
) -> Result<LocalCliFiles> {
    let prompt = format!(
        "{system}\n\nReturn only JSON matching the supplied schema. Do not wrap the JSON in markdown.\n\n{user}"
    );
    if prompt.len() > max_prompt_bytes {
        return Err(local_cli_extract_error(format!(
            "prompt is too large: {} bytes > {} byte limit",
            prompt.len(),
            max_prompt_bytes
        )));
    }
    if let Some(kind) = find_obvious_clear_pii(&prompt) {
        return Err(local_cli_extract_error(format!(
            "provider prompt contains obvious clear PII: {kind}"
        )));
    }

    tokio::fs::create_dir_all(dir).await?;
    let prompt_path = dir.join("prompt.txt");
    let schema_path = dir.join("schema.json");
    let output_path = dir.join("result.json");

    let schema = serde_json::to_string(json_schema)?;
    tokio::fs::write(&prompt_path, prompt.as_bytes()).await?;
    tokio::fs::write(&schema_path, schema.as_bytes()).await?;

    Ok(LocalCliFiles {
        prompt_hash: sha256_hex(prompt.as_bytes()),
        schema_hash: sha256_hex(schema.as_bytes()),
        prompt_path,
        schema_path,
        output_path,
    })
}

pub(crate) async fn read_output_json(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Err(local_cli_extract_error(format!(
            "provider did not write output JSON at {}",
            path.display()
        )));
    }
    let raw = tokio::fs::read_to_string(path).await?;
    let value = serde_json::from_str(&raw)?;
    Ok(value)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}
```

- [ ] **Step 4: Run local CLI tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: PASS.

- [ ] **Step 5: Commit Task 2**

```powershell
git add -- crates/anno-rag-tabular/src/llm/local_cli.rs
git commit -m "feat(tabular): add local cli request files"
```

## Task 3: Implement CodexLocalLlm Command Construction

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/codex_local.rs`
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`

- [ ] **Step 1: Write failing unit tests for Codex args and model id**

Create `crates/anno-rag-tabular/src/llm/codex_local.rs` with this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn command_args_omit_model_when_auto() {
        let cfg = CodexLocalConfig {
            binary: "codex".into(),
            model: None,
            timeout: Duration::from_secs(30),
            max_prompt_bytes: 1024,
        };
        let args = build_codex_exec_args(&cfg, Path::new("schema.json"), Path::new("result.json"));

        assert_eq!(args[0], "exec");
        assert!(args.iter().any(|arg| arg == "--ephemeral"));
        assert!(args.iter().any(|arg| arg == "--output-schema"));
        assert!(args.iter().any(|arg| arg == "-o"));
        assert!(!args.iter().any(|arg| arg == "--model"));
    }

    #[test]
    fn command_args_include_model_when_set() {
        let cfg = CodexLocalConfig {
            binary: "codex".into(),
            model: Some("gpt-test".into()),
            timeout: Duration::from_secs(30),
            max_prompt_bytes: 1024,
        };
        let args = build_codex_exec_args(&cfg, Path::new("schema.json"), Path::new("result.json"));

        let model_pos = args.iter().position(|arg| arg == "--model").expect("model flag");
        assert_eq!(args[model_pos + 1], "gpt-test");
    }

    #[test]
    fn model_id_mentions_local_codex() {
        let client = CodexLocalLlm::new(CodexLocalConfig::default());
        assert_eq!(client.model_id(), "codex-local:auto");
    }
}
```

- [ ] **Step 2: Export the module and run tests to verify failure**

Modify `crates/anno-rag-tabular/src/llm/mod.rs`:

```rust
pub mod anthropic;
pub mod codex_local;
pub(crate) mod local_cli;
pub mod mock;
```

Run:

```powershell
cargo test -p anno-rag-tabular llm::codex_local --lib
```

Expected: FAIL because `CodexLocalConfig`, `CodexLocalLlm`, and `build_codex_exec_args` are not implemented.

- [ ] **Step 3: Implement config, model id, and command args**

Add this implementation above the tests in `crates/anno-rag-tabular/src/llm/codex_local.rs`:

```rust
use super::{LlmClient, StructuredOutput};
use crate::error::Result;
use crate::llm::local_cli;
use async_trait::async_trait;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 180;
const DEFAULT_MAX_PROMPT_BYTES: usize = 800_000;

#[derive(Debug, Clone)]
pub struct CodexLocalConfig {
    pub binary: PathBuf,
    pub model: Option<String>,
    pub timeout: Duration,
    pub max_prompt_bytes: usize,
}

impl Default for CodexLocalConfig {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("codex"),
            model: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_prompt_bytes: DEFAULT_MAX_PROMPT_BYTES,
        }
    }
}

pub struct CodexLocalLlm {
    cfg: CodexLocalConfig,
    model_id: String,
}

impl CodexLocalLlm {
    #[must_use]
    pub fn new(cfg: CodexLocalConfig) -> Self {
        let model_id = match cfg.model.as_deref() {
            Some(model) if !model.is_empty() => format!("codex-local:{model}"),
            _ => "codex-local:auto".to_string(),
        };
        Self { cfg, model_id }
    }
}

pub(crate) fn build_codex_exec_args(
    cfg: &CodexLocalConfig,
    schema_path: &Path,
    output_path: &Path,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("exec"),
        OsString::from("--ephemeral"),
        OsString::from("--output-schema"),
        schema_path.as_os_str().to_os_string(),
        OsString::from("-o"),
        output_path.as_os_str().to_os_string(),
    ];

    if let Some(model) = cfg.model.as_deref().filter(|model| !model.is_empty()) {
        args.push(OsString::from("--model"));
        args.push(OsString::from(model));
    }

    args
}

#[async_trait]
impl LlmClient for CodexLocalLlm {
    async fn generate_structured(
        &self,
        _system: &str,
        _user: &str,
        _json_schema: &Value,
    ) -> Result<StructuredOutput> {
        Err(local_cli::local_cli_extract_error(
            "CodexLocalLlm command execution is not implemented yet",
        ))
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
```

- [ ] **Step 4: Run Codex local unit tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::codex_local --lib
```

Expected: PASS for the three command/model tests.

- [ ] **Step 5: Commit Task 3**

```powershell
git add -- crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/src/llm/codex_local.rs
git commit -m "feat(tabular): add codex local llm shell"
```

## Task 4: Execute Codex CLI and Parse Structured Output

**Files:**
- Modify: `crates/anno-rag-tabular/src/llm/local_cli.rs`
- Modify: `crates/anno-rag-tabular/src/llm/codex_local.rs`

- [ ] **Step 1: Add a test helper for fake executables**

Add this helper to the test module in `crates/anno-rag-tabular/src/llm/local_cli.rs`:

```rust
#[cfg(unix)]
fn make_fake_exe(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake exe");
    let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod");
    path
}

#[cfg(windows)]
fn make_fake_exe(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(format!("{name}.cmd"));
    std::fs::write(&path, format!("@echo off\r\n{body}\r\n")).expect("write fake exe");
    path
}
```

- [ ] **Step 2: Write failing tests for command execution and timeout**

Add these tests to `crates/anno-rag-tabular/src/llm/local_cli.rs`:

```rust
#[tokio::test]
async fn run_command_writes_stdin_and_reads_output_file() {
    let dir = tempfile::tempdir().expect("tempdir");

    #[cfg(windows)]
    let fake = make_fake_exe(
        dir.path(),
        "fake-codex",
        r#"set OUT=
:loop
if "%~1"=="" goto run
if "%~1"=="-o" (
  set OUT=%~2
  shift
  shift
  goto loop
)
shift
goto loop
:run
> "%OUT%" echo {"ok":true}
exit /b 0"#,
    );

    #[cfg(unix)]
    let fake = make_fake_exe(
        dir.path(),
        "fake-codex",
        r#"out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    out="$2"
    shift 2
  else
    shift
  fi
done
cat >/dev/null
printf '{"ok":true}' > "$out"
exit 0"#,
    );

    let files = write_request_files(
        dir.path(),
        "system",
        "user",
        &serde_json::json!({"type": "object"}),
        1024,
    )
    .await
    .expect("request files");

    let value = run_json_command(
        &fake,
        vec![
            std::ffi::OsString::from("exec"),
            std::ffi::OsString::from("-o"),
            files.output_path.as_os_str().to_os_string(),
        ],
        &files.prompt_path,
        &files.output_path,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("command succeeds");

    assert_eq!(value, serde_json::json!({"ok": true}));
}

#[tokio::test]
async fn run_command_reports_nonzero_exit() {
    let dir = tempfile::tempdir().expect("tempdir");

    #[cfg(windows)]
    let fake = make_fake_exe(dir.path(), "fake-fail", "echo nope 1>&2\r\nexit /b 7");

    #[cfg(unix)]
    let fake = make_fake_exe(dir.path(), "fake-fail", "echo nope >&2\nexit 7");

    let files = write_request_files(
        dir.path(),
        "system",
        "user",
        &serde_json::json!({"type": "object"}),
        1024,
    )
    .await
    .expect("request files");

    let err = run_json_command(
        &fake,
        vec![std::ffi::OsString::from("exec")],
        &files.prompt_path,
        &files.output_path,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect_err("nonzero exit fails");

    assert!(format!("{err}").contains("local provider exited with status"));
}

#[tokio::test]
async fn run_command_reports_timeout() {
    let dir = tempfile::tempdir().expect("tempdir");

    #[cfg(windows)]
    let fake = make_fake_exe(
        dir.path(),
        "fake-slow",
        "ping -n 6 127.0.0.1 > nul\r\nexit /b 0",
    );

    #[cfg(unix)]
    let fake = make_fake_exe(dir.path(), "fake-slow", "sleep 5\nexit 0");

    let files = write_request_files(
        dir.path(),
        "system",
        "user",
        &serde_json::json!({"type": "object"}),
        1024,
    )
    .await
    .expect("request files");

    let err = run_json_command(
        &fake,
        vec![std::ffi::OsString::from("exec")],
        &files.prompt_path,
        &files.output_path,
        std::time::Duration::from_millis(10),
    )
    .await
    .expect_err("timeout fails");

    assert!(format!("{err}").contains("local provider timed out"));
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: FAIL because `run_json_command` is not implemented.

- [ ] **Step 4: Implement `run_json_command`**

Add these imports to `crates/anno-rag-tabular/src/llm/local_cli.rs`:

```rust
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time;
```

Add this function below `read_output_json`:

```rust
pub(crate) async fn run_json_command(
    binary: &Path,
    args: Vec<OsString>,
    prompt_path: &Path,
    output_path: &Path,
    timeout: Duration,
) -> Result<Value> {
    let prompt = tokio::fs::read(prompt_path).await?;

    let mut cmd = Command::new(binary);
    cmd.args(args);
    cmd.env_clear();
    cmd.envs(sanitized_subscription_env(std::env::vars_os()));
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        local_cli_extract_error(format!("failed to start local provider {}: {e}", binary.display()))
    })?;
    child.kill_on_drop(true);

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&prompt).await?;
    }

    let output = time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| local_cli_extract_error("local provider timed out"))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(local_cli_extract_error(format!(
            "local provider exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    read_output_json(output_path).await
}
```

- [ ] **Step 5: Run local CLI tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: PASS.

- [ ] **Step 6: Replace CodexLocalLlm stub with real execution**

Modify `CodexLocalLlm::generate_structured` in `crates/anno-rag-tabular/src/llm/codex_local.rs`:

```rust
#[async_trait]
impl LlmClient for CodexLocalLlm {
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> Result<StructuredOutput> {
        let root = std::env::temp_dir().join(format!("anno-rag-tabular-codex-{}", uuid::Uuid::now_v7()));
        let files = local_cli::write_request_files(
            &root,
            system,
            user,
            json_schema,
            self.cfg.max_prompt_bytes,
        )
        .await?;
        let args = build_codex_exec_args(&self.cfg, &files.schema_path, &files.output_path);
        let value = local_cli::run_json_command(
            &self.cfg.binary,
            args,
            &files.prompt_path,
            &files.output_path,
            self.cfg.timeout,
        )
        .await;
        let _ = tokio::fs::remove_dir_all(&root).await;

        Ok(StructuredOutput {
            value: value?,
            usage: Default::default(),
        })
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
```

- [ ] **Step 7: Run Codex local unit tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::codex_local --lib
cargo test -p anno-rag-tabular llm::local_cli --lib
```

Expected: PASS.

- [ ] **Step 8: Commit Task 4**

```powershell
git add -- crates/anno-rag-tabular/src/llm/local_cli.rs crates/anno-rag-tabular/src/llm/codex_local.rs
git commit -m "feat(tabular): run codex local provider"
```

## Task 5: Add Explicit Provider Config Resolver

**Files:**
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`

- [ ] **Step 1: Write failing resolver tests**

Add these tests to `crates/anno-rag-tabular/src/llm/mod.rs` inside the existing test module:

```rust
#[test]
fn from_config_builds_codex_local_provider() {
    let cfg = LlmProviderConfig {
        provider: LlmProviderKind::CodexLocal,
        codex: codex_local::CodexLocalConfig {
            model: Some("gpt-test".into()),
            ..Default::default()
        },
    };

    let client = from_config(&cfg).expect("provider resolves");
    assert_eq!(client.model_id(), "codex-local:gpt-test");
}

#[test]
fn default_from_env_remains_anthropic_api_only() {
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    }
    let c = default_from_env().expect("env path must resolve");
    assert_eq!(c.model_id(), "claude-sonnet-4-6");
    unsafe {
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
```

Remove the older `default_from_env_picks_up_env_var` test or replace its body with the new `default_from_env_remains_anthropic_api_only` test to avoid duplicate coverage with the same unsafe environment mutation.

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag-tabular llm::tests --lib
```

Expected: FAIL because `LlmProviderConfig`, `LlmProviderKind`, and `from_config` do not exist.

- [ ] **Step 3: Implement config resolver**

Add these definitions above `default_from_env()` in `crates/anno-rag-tabular/src/llm/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderKind {
    AnthropicApi,
    CodexLocal,
}

#[derive(Debug, Clone)]
pub struct LlmProviderConfig {
    pub provider: LlmProviderKind,
    pub codex: codex_local::CodexLocalConfig,
}

impl Default for LlmProviderConfig {
    fn default() -> Self {
        Self {
            provider: LlmProviderKind::AnthropicApi,
            codex: codex_local::CodexLocalConfig::default(),
        }
    }
}

pub fn from_config(cfg: &LlmProviderConfig) -> crate::error::Result<Box<dyn LlmClient>> {
    match cfg.provider {
        LlmProviderKind::AnthropicApi => default_from_env(),
        LlmProviderKind::CodexLocal => Ok(Box::new(codex_local::CodexLocalLlm::new(cfg.codex.clone()))),
    }
}
```

Keep `default_from_env()` unchanged except for moving tests.

- [ ] **Step 4: Run resolver tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::tests --lib
```

Expected: PASS.

- [ ] **Step 5: Commit Task 5**

```powershell
git add -- crates/anno-rag-tabular/src/llm/mod.rs
git commit -m "feat(tabular): resolve local llm provider config"
```

## Task 6: Add Fake Codex Integration Test Through Extractor

**Files:**
- Create: `crates/anno-rag-tabular/tests/codex_local_fake.rs`

- [ ] **Step 1: Write the integration test**

Create `crates/anno-rag-tabular/tests/codex_local_fake.rs`:

```rust
use anno_rag_tabular::extract::{ChunkRef, ChunkSource, Extractor};
use anno_rag_tabular::llm::codex_local::{CodexLocalConfig, CodexLocalLlm};
use anno_rag_tabular::schema::{column::ColumnBuilder, CellType};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

struct OneChunkSource {
    chunk: ChunkRef,
}

#[async_trait]
impl ChunkSource for OneChunkSource {
    async fn chunks_for_doc(&self, _doc: Uuid) -> anno_rag_tabular::Result<Vec<ChunkRef>> {
        Ok(vec![self.chunk.clone()])
    }

    async fn chunk_by_id(&self, chunk_id: Uuid) -> anno_rag_tabular::Result<Option<ChunkRef>> {
        Ok((self.chunk.id == chunk_id).then(|| self.chunk.clone()))
    }
}

#[cfg(unix)]
fn make_fake_codex(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join("codex");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake codex");
    let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod");
    path
}

#[cfg(windows)]
fn make_fake_codex(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("codex.cmd");
    std::fs::write(&path, format!("@echo off\r\n{body}\r\n")).expect("write fake codex");
    path
}

#[tokio::test]
async fn codex_local_provider_feeds_existing_extractor() {
    let temp = tempfile::tempdir().expect("tempdir");
    let review = anno_rag_tabular::ReviewId::new();
    let doc = Uuid::new_v4();
    let chunk_id = Uuid::new_v4();
    let chunk_text = "PERSON_1 signe le contrat.";

    #[cfg(windows)]
    let fake = make_fake_codex(
        temp.path(),
        &format!(
            r#"set OUT=
:loop
if "%~1"=="" goto run
if "%~1"=="-o" (
  set OUT=%~2
  shift
  shift
  goto loop
)
shift
goto loop
:run
> "%OUT%" echo {{"party":{{"value":"PERSON_1","reasoning":"Le signataire est cite dans le chunk.","citations":[{{"chunk_id":"{chunk_id}","byte_start":0,"byte_end":8,"quoted_text":"PERSON_1"}}]}}}}
exit /b 0"#
        ),
    );

    #[cfg(unix)]
    let fake = make_fake_codex(
        temp.path(),
        &format!(
            r#"out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    out="$2"
    shift 2
  else
    shift
  fi
done
cat >/dev/null
printf '{{"party":{{"value":"PERSON_1","reasoning":"Le signataire est cite dans le chunk.","citations":[{{"chunk_id":"{chunk_id}","byte_start":0,"byte_end":8,"quoted_text":"PERSON_1"}}]}}}}' > "$out"
exit 0"#
        ),
    );

    let llm = Arc::new(CodexLocalLlm::new(CodexLocalConfig {
        binary: fake,
        model: None,
        timeout: Duration::from_secs(5),
        max_prompt_bytes: 32_000,
    }));
    let chunks = Arc::new(OneChunkSource {
        chunk: ChunkRef {
            id: chunk_id,
            doc_id: doc,
            content: chunk_text.to_string(),
            page: Some(1),
        },
    });
    let extractor = Extractor::new(llm, chunks);
    let col = ColumnBuilder::new(review, "party", "Extract the signing party.", CellType::Text)
        .build();

    let cells = extractor
        .extract_doc(review, doc, &[col])
        .await
        .expect("extracts through fake codex");

    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].value, serde_json::json!("PERSON_1"));
    assert_eq!(cells[0].citations[0].quoted_text, "PERSON_1");
}
```

- [ ] **Step 2: Run the fake integration test**

Run:

```powershell
cargo test -p anno-rag-tabular --test codex_local_fake
```

Expected: PASS. If Windows command quoting fails, inspect the generated `.cmd` file and adjust only the fake test script, not production code.

- [ ] **Step 3: Commit Task 6**

```powershell
git add -- crates/anno-rag-tabular/tests/codex_local_fake.rs
git commit -m "test(tabular): cover codex local extractor"
```

## Task 7: Add Ignored Live Codex Smoke Test

**Files:**
- Create: `crates/anno-rag-tabular/tests/codex_local_live.rs`

- [ ] **Step 1: Write ignored live test**

Create `crates/anno-rag-tabular/tests/codex_local_live.rs`:

```rust
use anno_rag_tabular::llm::codex_local::{CodexLocalConfig, CodexLocalLlm};
use anno_rag_tabular::llm::LlmClient;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires installed Codex CLI with local subscription login"]
async fn live_codex_local_structured_output() {
    if std::env::var("RUN_CODEX_LOCAL_LIVE").ok().as_deref() != Some("1") {
        eprintln!("set RUN_CODEX_LOCAL_LIVE=1 to run this live test");
        return;
    }

    let client = CodexLocalLlm::new(CodexLocalConfig {
        binary: "codex".into(),
        model: None,
        timeout: Duration::from_secs(120),
        max_prompt_bytes: 32_000,
    });

    let out = client
        .generate_structured(
            "Return only JSON.",
            "Return an object with ok=true.",
            &serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "ok": { "type": "boolean" }
                },
                "required": ["ok"]
            }),
        )
        .await
        .expect("codex local live call succeeds");

    assert_eq!(out.value, serde_json::json!({"ok": true}));
}
```

- [ ] **Step 2: Verify ignored test compiles without running live provider**

Run:

```powershell
cargo test -p anno-rag-tabular --test codex_local_live
```

Expected: PASS with one ignored test.

- [ ] **Step 3: Document the live test command in the test file header**

Add this module doc comment to the top of `crates/anno-rag-tabular/tests/codex_local_live.rs`:

```rust
//! Ignored live smoke test for a locally authenticated Codex CLI.
//!
//! Run manually after `codex login`:
//!
//! ```powershell
//! $env:RUN_CODEX_LOCAL_LIVE='1'
//! cargo test -p anno-rag-tabular --test codex_local_live -- --ignored --nocapture
//! ```
```

- [ ] **Step 4: Commit Task 7**

```powershell
git add -- crates/anno-rag-tabular/tests/codex_local_live.rs
git commit -m "test(tabular): add codex local live smoke"
```

## Task 8: Final Verification

**Files:**
- No new files.

- [ ] **Step 1: Run focused tests**

Run:

```powershell
cargo test -p anno-rag-tabular llm::local_cli --lib
cargo test -p anno-rag-tabular llm::codex_local --lib
cargo test -p anno-rag-tabular llm::tests --lib
cargo test -p anno-rag-tabular --test codex_local_fake
cargo test -p anno-rag-tabular --test codex_local_live
```

Expected: all pass; `codex_local_live` reports ignored unless explicitly run with `--ignored`.

- [ ] **Step 2: Run package check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
```

Expected: PASS.

- [ ] **Step 3: Verify provider does not forward API key environment variables**

Run:

```powershell
cargo test -p anno-rag-tabular subscription_env_strips_api_key_variables --lib
```

Expected: PASS.

- [ ] **Step 4: Verify GitNexus changed scope**

The MCP `gitnexus_detect_changes` tool is not exposed in this Codex session. Use the available CLI and Git fallback:

```powershell
npx gitnexus status
git diff --name-status HEAD~7..HEAD
```

Expected changed files for this implementation slice:

```text
crates/anno-rag-tabular/src/llm/mod.rs
crates/anno-rag-tabular/src/llm/local_cli.rs
crates/anno-rag-tabular/src/llm/codex_local.rs
crates/anno-rag-tabular/tests/codex_local_fake.rs
crates/anno-rag-tabular/tests/codex_local_live.rs
```

- [ ] **Step 5: Run security review checklist**

Manually verify these facts in the final diff:

```text
No code reads ~/.codex/auth.json.
No code reads Claude credentials.
No code stores provider tokens.
No code forwards OPENAI_API_KEY, CODEX_API_KEY, ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN, or ANTHROPIC_BASE_URL in subscription mode.
No production test requires a live Codex login.
No prompt file is persisted after successful provider execution.
```

- [ ] **Step 6: Commit any final fixes**

If verification required fixes:

```powershell
git add -- crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/src/llm/local_cli.rs crates/anno-rag-tabular/src/llm/codex_local.rs crates/anno-rag-tabular/tests/codex_local_fake.rs crates/anno-rag-tabular/tests/codex_local_live.rs
git commit -m "fix(tabular): harden codex local provider"
```

If no fixes were needed, do not create an empty commit.

## Handoff Notes

After this plan is implemented, users can configure a `CodexLocalLlm` from explicit config and run structured extraction through their local Codex CLI login without API keys. The next separate implementation plans should cover:

- Codex app-server JSON-RPC provider;
- Claude local provider with product-gating;
- insight extraction schemas and storage;
- UI/provider settings in the Tauri workbench.
