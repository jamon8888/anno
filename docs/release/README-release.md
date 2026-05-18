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
Select-String -Path .\SHA256SUMS.txt -Pattern 'hacienda-<tag>-x86_64-pc-windows-msvc.zip'
(Get-FileHash .\hacienda-<tag>-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash.ToLowerInvariant()
```

macOS:

```sh
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
expected="$(grep 'hacienda-<tag>-aarch64-apple-darwin.tar.gz' SHA256SUMS.txt | awk '{print $1}')"
actual="$(shasum -a 256 hacienda-<tag>-aarch64-apple-darwin.tar.gz | awk '{print $1}')"
test "$expected" = "$actual"
```
