# Hacienda Release Install

This document explains how to install the GitHub Release binary archives for Claude Desktop and local HTTP gateway use.

## Pick the Right Asset

| Platform | Asset |
|---|---|
| Windows 11 x64 | `hacienda-<tag>-x86_64-pc-windows-msvc.zip` |
| macOS Intel | `hacienda-<tag>-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `hacienda-<tag>-aarch64-apple-darwin.tar.gz` |

Each archive contains:

- `anno-rag` or `anno-rag.exe`
- `anno-privacy-gateway` or `anno-privacy-gateway.exe`
- `README.md`
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `env.example`
- `examples/claude_desktop_config.windows.json`
- `examples/claude_desktop_config.macos.json`

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

## Claude Desktop

Claude Desktop uses the `anno-rag` binary through stdio MCP:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/absolute/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

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

## First Run and Offline Mode

The release archives do not contain model weights.

For best first-run behavior, run the warmup command from a source checkout or development build before setting `ANNO_NO_DOWNLOADS=1`:

```sh
cargo run --release --example warmup_model -p anno-rag
```

If models are already in the HuggingFace cache, `ANNO_NO_DOWNLOADS=1` keeps runtime operation offline.

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
