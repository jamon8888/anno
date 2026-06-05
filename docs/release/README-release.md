# Hacienda Release Install

This document explains how to install the GitHub Release binary archives for
Claude Desktop/Cowork, Claude Code, and local HTTP gateway use.

Current candidate promoted as GitHub "Latest": `v0.11.0-rc.11`.

## Pick the Right Asset

| Platform | Asset |
|---|---|
| Windows 11 x64 | `hacienda-<tag>-x86_64-pc-windows-msvc.zip` |
| macOS Intel | `hacienda-<tag>-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `hacienda-<tag>-aarch64-apple-darwin.tar.gz` |

For new tags, the canonical CPU release path is the `Release` workflow generated
by cargo-dist. It publishes platform archives/installers, checksums, and `.mcpb`
extension bundles. The `Release Binaries (Manual Fallback)` workflow is manual
only and publishes archive/checksum assets if the cargo-dist path needs a
targeted rerun.

Each archive contains:

- `anno-rag` or `anno-rag.exe`
- `anno-privacy-gateway` or `anno-privacy-gateway.exe`
- `README.md`
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `env.example`
- `examples/claude_desktop_config.windows.json`
- `examples/claude_desktop_config.macos.json`
- `scripts/setup-mcp.ps1`
- `scripts/setup-mcp.sh`

## Pre-Release Local Pipeline Gate

Before building or publishing OS assets, run the local gate on a representative
folder:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\local-pipeline-gate.ps1 -Profile fast
```

Profiles:

- `fast`: check, reuse/build local debug binaries when available, small local
  sample ingest, re-ingest skip smoke, search smoke, and gateway boot smoke.
  It compile-checks OCR support but skips test binary linking, OCR runtime
  work, and release optimization.
- `release`: `fast` plus model-heavy PII NER, resumable ingest, and
  `anno-rag bench`; OCR runtime gates are enabled unless `-SkipOcr` is passed.
- `deep`: `release` plus rerank/eval/memory benches.

The gate writes artifacts under `target/local-release-gate/run-*/`:

- `reports/metrics.json`
- `reports/report.md`
- `reports/commands.log`
- per-command logs under `logs/`

The fast runtime gate also smoke-tests `anno-privacy-gateway` by starting it
briefly on an ephemeral localhost port and stopping it automatically.
Each run sets `ANNO_RAG_DATA_DIR` to its own artifact directory so ingest,
re-ingest, and search are measured against an isolated local store.

### Pre-Tag Acceptance Metrics

Before pushing a release tag, run:

```powershell
cargo build -p anno-rag-bin -p anno-privacy-gateway
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\local-pipeline-gate.ps1 -Profile fast -SkipMcp -SkipBuild
```

Acceptance:

- `reports/metrics.json` has `"summary": { "status": "passed" }`.
- `anno-rag ingest local samples` ingests exactly `10` documents from the built-in `11` sample files. The duplicate sample is expected to deduplicate.
- `anno-rag reingest idempotency smoke` ingests exactly `0` documents.
- Each search smoke exits `0`.
- `anno-privacy-gateway boot smoke` exits `0`.
- No `anno-rag`, `anno_rag`, `anno-privacy-gateway`, `cargo`, or `rustc` process remains after the gate.

The latest known Windows debug baseline was `400.95s` for fresh ingest and
`1.23s` for re-ingest. Treat fresh ingest above `900s`, re-ingest above `10s`,
or any search above `90s` as a regression to investigate before tagging.

Use `-DryRun` to inspect the command plan without building, downloading models,
or creating sample data:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release\local-pipeline-gate.ps1 -DryRun -SkipHeavy -SkipOcr -SkipMcp
```

## Claude Desktop / Cowork / Claude Code

Use the setup wrappers from the extracted release archive as the primary install
path.

Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target all -Tag latest
```

macOS:

```bash
./scripts/setup-mcp.sh --target all --tag latest
```

Both wrappers delegate to the installed binary subcommand:

```bash
anno-rag setup-mcp --target all
```

`all` means Desktop/Cowork plus Claude Code. Use the manual instructions below
only when you need to inspect or edit the generated config yourself.

### Manual Assistant Install Prompt

Open Claude Code and paste:

```text
Install Hacienda anno-rag v0.11.0-rc.11 into Claude Desktop/Cowork and Claude Code from https://github.com/jamon8888/anno/releases/tag/v0.11.0-rc.11.
Download the asset for this machine (Windows x64: hacienda-v0.11.0-rc.11-x86_64-pc-windows-msvc.zip; macOS Intel: hacienda-v0.11.0-rc.11-x86_64-apple-darwin.tar.gz; macOS Apple Silicon: hacienda-v0.11.0-rc.11-aarch64-apple-darwin.tar.gz) plus SHA256SUMS.txt, verify the checksum, extract it to a stable local folder, and update Claude Desktop's claude_desktop_config.json so mcpServers.anno-rag runs the extracted anno-rag binary with args ["mcp"]. If Claude Code is installed, also run claude mcp add --transport stdio --scope user with the same binary and ANNO_MODELS_DIR. If models are not already installed, run anno-rag download-models once and set ANNO_MODELS_DIR to the path it prints. Do not add ANNO_RAG_VAULT_PASSPHRASE unless I provide one. After editing the config, tell me to fully restart Claude Desktop/Cowork and verify anno-rag appears under Connectors.
```

### Manual Desktop/Cowork config

Claude Desktop and Cowork-in-Desktop use the `anno-rag` binary through stdio
MCP:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/absolute/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "/absolute/path/to/.anno-rag/models"
      }
    }
  }
}
```

Replace `/absolute/path/to/.anno-rag/models` with the path printed by `anno-rag download-models`.
`ANNO_NO_DOWNLOADS=1` still works as a fallback if models are already in the HuggingFace cache.

Config file locations:

- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`

Rules:

- Use an absolute path to the extracted `anno-rag` binary.
- Escape Windows backslashes in JSON.
- Restart Claude Desktop after editing the config.
- Verify `anno-rag` appears under Claude Desktop Connectors.
- By default, omit `ANNO_RAG_VAULT_PASSPHRASE` so `anno-rag` uses the OS keyring for vault encryption.
- Advanced users may add `ANNO_RAG_VAULT_PASSPHRASE` locally with a strong, unique secret. JSON does not support comments, so keep this note outside the config file.

### Manual Claude Code config

Use the Claude Code CLI for Claude Code MCP configuration:

```powershell
claude mcp add --transport stdio --scope user `
  --env ANNO_MODELS_DIR=C:\Users\you\.anno-rag\models `
  anno-rag -- C:\Users\you\Tools\hacienda-v0.11.0-rc.11\anno-rag.exe mcp
```

macOS:

```bash
claude mcp add --transport stdio --scope user \
  --env ANNO_MODELS_DIR="$HOME/.anno-rag/models" \
  anno-rag -- "$HOME/Tools/hacienda-v0.11.0-rc.11/anno-rag" mcp
```

Use `claude mcp list` and `/mcp` in Claude Code to verify the server. Use
`--scope project` only when you intentionally want a project-level `.mcp.json`.

## First Run and Offline Mode

The release archives do not contain model weights (~970 MiB total).
Run the one-time download command included with the binary:

```sh
anno-rag download-models
```

This downloads both models (intfloat/multilingual-e5-small + SemplificaAI/gliner2-multi-v1-onnx)
to `~/.anno-rag/models` and prints the path. Add the printed path to your environment:

```sh
# macOS / Linux — add to ~/.bashrc or ~/.zshrc
export ANNO_MODELS_DIR="$HOME/.anno-rag/models"

# Windows PowerShell — persistent, current user
[System.Environment]::SetEnvironmentVariable("ANNO_MODELS_DIR", "$env:USERPROFILE\.anno-rag\models", "User")
```

After setting `ANNO_MODELS_DIR`, anno-rag starts without any network call.

> **Developers**: the warmup example still works too — `cargo run --release --example warmup_model -p anno-rag` downloads to the HuggingFace cache (`~/.cache/huggingface/hub/`). If models are already in that cache, `ANNO_NO_DOWNLOADS=1` keeps runtime operation offline. Use `anno-rag download-models` for end-user installs and `warmup_model` for development.

## OCR

Tesseract is optional and is not bundled.

- Windows: `winget install --id UB-Mannheim.TesseractOCR`
- macOS: `brew install tesseract tesseract-lang`

For release binaries, `tesseract` must be on `PATH` for OCR. A custom `tesseract_path` requires source/config support and is not part of the release install flow.

## Checksums

Download `SHA256SUMS.txt` from the release and verify the archive before extracting it.

Windows PowerShell:

```powershell
$asset = "hacienda-<tag>-x86_64-pc-windows-msvc.zip"
$expected = (Select-String -Path .\SHA256SUMS.txt -Pattern $asset).Line.Split()[0]
$actual = (Get-FileHash .\$asset -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "checksum mismatch for $asset" }
"checksum ok: $asset"
```

macOS:

```sh
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
expected="$(grep 'hacienda-<tag>-aarch64-apple-darwin.tar.gz' SHA256SUMS.txt | awk '{print $1}')"
actual="$(shasum -a 256 hacienda-<tag>-aarch64-apple-darwin.tar.gz | awk '{print $1}')"
test "$expected" = "$actual"
```

## RC release flow for Cowork performance testing

Use this flow to create an optimized GitHub candidate release for Claude Desktop/Cowork testing.

### Preconditions

- `origin/main` equals local `main`.
- Latest GitHub Actions CI on `main` is successful.
- `v0.11.0-rc.11` or the target tag points at the intended commit.
- No local Claude Desktop MCP process is still expected to run from `D:\cargo-shared-target\debug\anno-rag.exe` after install.

### Create the RC

```powershell
git tag v0.11.0-rc.11
git push origin main
git push origin v0.11.0-rc.11
gh run list --repo jamon8888/anno --workflow "Release" --limit 5
```

Monitor the selected release run:

```powershell
$Run = gh run list --repo jamon8888/anno --workflow "Release" --limit 5 --json databaseId,displayTitle |
  ConvertFrom-Json |
  Where-Object { $_.displayTitle -match 'v0\.11\.0-rc\.11' } |
  Select-Object -First 1
gh run view $Run.databaseId --repo jamon8888/anno --json status,conclusion,url,jobs
```

If the cargo-dist release path fails because of a workflow-specific packaging
issue, run `Release Binaries (Manual Fallback)` manually against the same tag.
Do not use the fallback as the default tag trigger.

### Install in Cowork

Extract the Windows release archive and point Claude Desktop at the extracted
`anno-rag.exe` with args `["mcp"]`.

After installing, restart Claude Desktop/Cowork and verify:

```powershell
Get-Process anno-rag -ErrorAction SilentlyContinue |
  Select-Object Id,Path,StartTime,CPU,WorkingSet64
```

The `Path` must not contain `\debug\`.

### Capture performance evidence

Record these values before promoting the RC:

- MCP process path.
- `anno_health` response.
- First tool-call latency after restart.
- Representative ingest latency.
- Representative search latency.
- Peak working set for `anno-rag.exe`.
- Relevant lines from `C:\Users\NMarchitecte\AppData\Roaming\Claude\logs\mcp-server-anno-rag.log`.
