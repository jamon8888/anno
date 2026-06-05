# Claude Desktop, Cowork, And Claude Code Setup

Status: Setup helper available in v0.11.0-rc.11
Audience: User, Integrator, Admin
Language: Bilingual

Claude Desktop, Cowork, and Claude Code connect to Hacienda through the local
stdio MCP server:

```powershell
anno-rag mcp
```

The MCP client starts this command for you. The server exposes local tools such
as `anno_health`, `download_models`, `search`, `rehydrate`, `detect`,
`vault_stats`, memory tools, and legal review tools.

Claude Desktop et Cowork partagent la meme cible locale quand Cowork est utilise
dans Claude Desktop. Claude Code se configure separement, mais pointe vers le
meme binaire local et le meme cache de modeles.

## Client Targets

| Target | Covers | Configuration path |
|---|---|---|
| `desktop` | Claude Desktop + Cowork in Claude Desktop | `.mcpb` extension or `claude_desktop_config.json` |
| `claude-code` | Claude Code CLI | `claude mcp add` |
| `all` | Desktop/Cowork + Claude Code | Both of the above |

The cross-platform setup helper is available through the release wrapper scripts
or the installed binary:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-mcp.ps1 -Target all
```

```bash
./scripts/setup-mcp.sh --target all
```

The binary subcommand used by both wrappers is:

```bash
anno-rag setup-mcp --target all
```

Use the manual steps below only when you need to inspect or edit the generated
configuration yourself.

## Claude Desktop Config

Claude Desktop usually reads MCP server definitions from:

| OS | Config path |
|---|---|
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |

Cowork running inside Claude Desktop uses the same `mcpServers` entry because
Claude Desktop launches the local MCP process.

## Windows Example

Use an absolute path to the extracted release binary:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\Users\\you\\Tools\\hacienda-v0.11.0-rc.11\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "C:\\Users\\you\\.anno-rag\\models"
      }
    }
  }
}
```

## macOS Example

Use an absolute path to the extracted release binary:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/Users/you/Tools/hacienda-v0.11.0-rc.11/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "/Users/you/.anno-rag/models"
      }
    }
  }
}
```

Restart Claude Desktop or Cowork after changing the config.

## Claude Code Config

Claude Code should be configured with the CLI instead of editing its config
files by hand. Use a user-scoped server for normal Hacienda installs:

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

Use `--scope project` only when you intentionally want to write a project-level
`.mcp.json`. Use `claude mcp list` and `/mcp` inside Claude Code to verify the
server is connected.

## Vault Secret Guidance

Prefer the OS keyring for the vault key. In normal local installs, do not set
`ANNO_RAG_VAULT_PASSPHRASE` manually.

Use `ANNO_RAG_VAULT_PASSPHRASE` only when an admin explicitly manages the
secret through an approved environment or secrets system. Never paste it into
chat, commit it, log it, or share screenshots that reveal it.

For first setup through MCP, call `anno_init_vault` only if you intentionally
want to provide your own passphrase. Otherwise let the local keyring-backed
path initialize the vault.

## Manual Assistant Install Prompt

Use this prompt in Claude Code or Cowork-in-Desktop when you want the assistant
to perform the release install manually instead of using the setup helper:

```text
Install Hacienda v0.11.0-rc.11 for Claude Desktop/Cowork on this machine.

Download the matching archive from:
https://github.com/jamon8888/anno/releases/tag/v0.11.0-rc.11

Also download SHA256SUMS.txt, verify the archive checksum, extract it to a
stable local folder, and update the MCP configuration so mcpServers.anno-rag
runs the extracted anno-rag binary with args ["mcp"].

If models are not installed, run anno-rag download-models once and set
ANNO_MODELS_DIR to the printed models path in the MCP server env.

Prefer the OS keyring for the vault. Do not set ANNO_RAG_VAULT_PASSPHRASE
unless I explicitly provide and manage one. Never log or display any vault
secret.

After editing the Desktop/Cowork config and Claude Code config when available,
ask me to fully restart Claude Desktop/Cowork, then verify by calling
anno_health and download_models from the MCP tools.
```

## Verification

After restart, ask Claude Desktop or Cowork:

```text
Call the anno_health tool and summarize the Hacienda version, build target,
available tools, and vault status.
```

In Claude Code, run `claude mcp list`, then use `/mcp` in an interactive session
to confirm `anno-rag` is connected.

If `anno_health` is not visible, the MCP server did not start or the client did
not reload the config. Model readiness is verified separately by calling
`download_models` or by running the first ingest/search workflow.

## Next Step

Create a first local corpus and search it:
[First Index And Search](first-index.md).
