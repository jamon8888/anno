# Cross-Platform MCP Setup for Claude Desktop, Cowork, and Claude Code

**Date:** 2026-06-04
**Status:** Accepted and implemented in `anno-rag setup-mcp`
**Scope:** Local MCP installation and client configuration automation. No change
to MCP tool behavior, indexing behavior, vault semantics, or model inference.

## 1. Goal

Provide one simple cross-platform setup flow that installs `anno-rag`, downloads
the local model cache, and connects the MCP server to the Claude clients used by
Hacienda users.

The setup must serve three local client targets:

1. **Claude Desktop / Cowork** - one target. In the Hacienda product model,
   Cowork is used inside Claude Desktop and therefore shares the same local MCP
   configuration as Claude Desktop.
2. **Claude Code** - configured through the `claude mcp add` command when the
   Claude Code CLI is available.
3. **Manual or managed MCP clients** - supported by printing the exact JSON
   snippet and paths instead of modifying a client config.

The user-facing command is:

```text
anno-rag setup-mcp --target all
```

or, from the repository during development:

```text
scripts/setup-mcp.ps1 -Target all
scripts/setup-mcp.sh --target all
```

## 2. Product Contract

`anno-rag mcp` remains the only local stdio server command:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/absolute/path/to/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "/absolute/path/to/models"
      }
    }
  }
}
```

Desktop and Cowork are configured together by updating the Claude Desktop local
MCP configuration or by installing the platform `.mcpb` extension. Claude Code is
configured separately because it owns its own MCP scopes and CLI.

## 3. Current Facts

- `anno-rag mcp` starts the stdio MCP server.
- `anno-rag download-models` is the canonical command for downloading the local
  embedder and GLiNER model cache.
- Release archives do not bundle model weights, currently about 970 MiB total.
- Release assets already include OS-specific archives and `.mcpb` extension
  packaging.
- Claude Desktop supports local MCP through Desktop Extensions (`.mcpb`) and
  manual JSON configuration.
- Claude Code supports local stdio MCP servers through `claude mcp add`.
- A script-only JSON patch is easy on Windows PowerShell but fragile on
  macOS/Linux unless it depends on Python, `jq`, or Node.

## 4. Non-Goals

- Do not build the full Rust workspace by default.
- Do not bundle model weights inside GitHub release archives or `.mcpb` files.
- Do not write `ANNO_RAG_VAULT_PASSPHRASE` by default.
- Do not overwrite other MCP servers in the same client config.
- Do not implement a cloud-hosted remote MCP connector in this track.
- Do not require Python, `jq`, Node, or Homebrew for the release install path.

## 5. Recommended Architecture

Use thin OS wrappers plus a small Rust configuration subcommand.

```text
scripts/setup-mcp.ps1       # Windows wrapper
scripts/setup-mcp.sh        # macOS/Linux wrapper
anno-rag setup-mcp          # cross-platform config writer and smoke checks
```

The wrappers handle platform download concerns:

1. Detect OS and architecture.
2. Resolve the release asset for `--tag latest` or a specific tag.
3. Download the archive and `SHA256SUMS.txt`.
4. Verify SHA-256.
5. Extract `anno-rag` into a stable install directory.
6. Call the installed binary's `setup-mcp` subcommand.

The Rust subcommand handles configuration safely:

1. Validate absolute binary and model paths.
2. Download models unless `--skip-models` is set.
3. Merge the Desktop/Cowork `mcpServers.anno-rag` entry.
4. Run `claude mcp add` for Claude Code when requested and available.
5. Create backups before writing config.
6. Perform a fast MCP health smoke.
7. Print restart and verification instructions.

This keeps JSON manipulation inside `serde_json`, avoids shell-specific escaping
bugs, and avoids external dependencies on user machines.

## 6. Command Shape

### 6.1 Wrapper scripts

Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 `
  -Target all `
  -Tag latest
```

macOS/Linux:

```bash
./scripts/setup-mcp.sh --target all --tag latest
```

Development install from the current workspace:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 `
  -Target desktop `
  -Source local-build `
  -SkipModels
```

### 6.2 `anno-rag setup-mcp`

Available CLI:

```text
anno-rag setup-mcp
  --target desktop|claude-code|all|manual
  --binary /absolute/path/to/anno-rag
  --models-dir /absolute/path/to/models
  --desktop-config /optional/path/to/claude_desktop_config.json
  --desktop-mode json|mcpb
  --claude-code-scope local|user|project
  --skip-models
  --dry-run
  --force
```

Defaults:

| Option | Default |
|---|---|
| `--target` | `all` |
| `--desktop-mode` | `json` |
| `--claude-code-scope` | `user` |
| `--models-dir` | platform default equivalent to `~/.anno-rag/models` |
| `--force` | false |

`manual` never writes config, downloads models, runs Claude Code, or starts the
MCP smoke. It validates paths, inspects the current model cache state, and
prints the Desktop JSON and Claude Code command.

## 7. Desktop / Cowork Configuration

The Desktop/Cowork target writes or updates:

- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`

Rules:

- Preserve all existing top-level fields.
- Preserve all existing `mcpServers` except `anno-rag`.
- Backup the original file as `claude_desktop_config.json.bak.<timestamp>`.
- Write atomically through a temporary file and rename.
- Use absolute `command` and `ANNO_MODELS_DIR` paths.
- Do not set `ANNO_RAG_VAULT_PASSPHRASE`.
- Add `ANNO_NO_DOWNLOADS=1` only after the model cache has been verified.
- Print a clear restart instruction for Claude Desktop and Cowork.

The `.mcpb` path remains useful for a user-friendly Desktop extension install,
but the automated setup should default to JSON because it is scriptable and
testable.

## 8. Claude Code Configuration

When `claude` is available, the setup should configure Claude Code with:

```powershell
claude mcp add --transport stdio --scope user `
  --env ANNO_MODELS_DIR=C:\Users\you\.anno-rag\models `
  anno-rag -- C:\Users\you\Tools\hacienda\anno-rag.exe mcp
```

macOS/Linux:

```bash
claude mcp add --transport stdio --scope user \
  --env ANNO_MODELS_DIR="$HOME/.anno-rag/models" \
  anno-rag -- "$HOME/Tools/hacienda/anno-rag" mcp
```

If `claude` is not available, the setup must not fail the Desktop/Cowork target.
It should print the exact command to run later.

Default scope is `user`, because Hacienda is a user-level local tool and should
not write `.mcp.json` into arbitrary client projects unless the operator passes
`--claude-code-scope project`.

## 9. Model Handling

The setup owns first-run model readiness:

1. Run `anno-rag download-models --dir <models-dir>` unless `--skip-models` or
   `--dry-run`.
2. Verify the expected E5 and GLiNER files, not just non-empty directories:
   `multilingual-e5-small/{config.json,tokenizer.json,model.safetensors}` plus
   the GLiNER ONNX tokenizer and graph set under `gliner2-multi-v1-onnx`
   (`fp32_v2` or `fp16_v2`).
3. Put the same `ANNO_MODELS_DIR` in Desktop/Cowork and Claude Code config.
4. Set `ANNO_NO_DOWNLOADS=1` only when the cache is verified.

This avoids the previous failure mode where Claude starts the MCP server, the
first tool call triggers a large model download, and the user interprets the MCP
as broken or slow.

## 10. Validation

Fast setup validation:

1. Start `anno-rag mcp` over stdio with a temporary vault passphrase.
2. Send MCP `initialize`.
3. Send `notifications/initialized`.
4. Call `tools/list`.
5. Confirm `anno_health` is advertised.

Full validation remains separate and can reuse `scripts/mcp_full_smoke.py`:

```text
scripts/mcp-full-smoke.ps1
```

The setup should not index a user folder during install. Indexing belongs to the
first corpus workflow.

## 11. Safety And Privacy

- Never write vault passphrases by default.
- Never log secrets.
- Never silently delete or rewrite unrelated MCP servers.
- Avoid shell-built JSON.
- Use absolute paths everywhere.
- Keep the model cache outside temporary directories.
- Prefer the OS keyring for normal local vault use.
- Print every config path changed.

## 12. Implementation Phases

### Phase 1 - Documentation and manual contract

- Add this spec.
- Update README and getting-started docs.
- Make Desktop/Cowork and Claude Code targets explicit.

### Phase 2 - Scriptable setup

- Add `anno-rag setup-mcp`.
- Add `scripts/setup-mcp.ps1`.
- Add `scripts/setup-mcp.sh`.
- Add fast MCP health smoke support if the existing full smoke is too heavy.

### Phase 3 - Release integration

- Include setup scripts in release archives.
- Document install commands in release notes.
- Add CI checks that the scripts parse and the generated JSON is valid.

### Phase 4 - Extension polish

- Keep `.mcpb` for Claude Desktop users who prefer one-click install.
- Make setup output detect and recommend `.mcpb` when the user chooses
  interactive Desktop installation.

## 13. Acceptance Criteria

- A Windows user can install from a release archive, download models, configure
  Desktop/Cowork, and configure Claude Code without compiling Rust.
- A macOS user can do the same on Intel or Apple Silicon.
- Existing MCP config entries survive setup unchanged.
- `--dry-run` shows every planned write without modifying files.
- `--skip-models` works for offline or pre-provisioned environments.
- The setup can be re-run idempotently to point clients at a newer binary.
- `anno_health` is visible through the configured MCP client after restart.

## 14. References

- Claude Desktop local MCP and Desktop Extensions:
  https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop
- Claude Code MCP configuration:
  https://code.claude.com/docs/en/mcp
- Existing release install guide:
  `docs/release/README-release.md`
- Existing MCP smoke:
  `scripts/mcp_full_smoke.py`
