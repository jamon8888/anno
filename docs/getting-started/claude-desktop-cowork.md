# Claude Desktop And Cowork Setup

Status: Available in v0.11.0-rc.11
Audience: User, Integrator, Admin
Language: Bilingual

Claude Desktop and Cowork connect to Hacienda through the local stdio MCP
server:

```powershell
anno-rag mcp
```

The MCP client starts this command for you. The server exposes local tools such
as `anno_health`, `download_models`, `search`, `rehydrate`, `detect`,
`vault_stats`, memory tools, and legal review tools.

Claude Desktop et Cowork ne recoivent pas de service cloud Hacienda. Ils
lancent un binaire local, qui garde le vault, l'index et les modeles sur la
machine de l'utilisateur.

## Claude Desktop Config

Claude Desktop usually reads MCP server definitions from:

| OS | Config path |
|---|---|
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |

Cowork deployments can use the same `mcpServers` shape when they accept a local
MCP server definition.

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

## Vault Secret Guidance

Prefer the OS keyring for the vault key. In normal local installs, do not set
`ANNO_RAG_VAULT_PASSPHRASE` manually.

Use `ANNO_RAG_VAULT_PASSPHRASE` only when an admin explicitly manages the
secret through an approved environment or secrets system. Never paste it into
chat, commit it, log it, or share screenshots that reveal it.

For first setup through MCP, call `anno_init_vault` only if you intentionally
want to provide your own passphrase. Otherwise let the local keyring-backed
path initialize the vault.

## Claude Code Install Prompt

Use this prompt in Claude Code or Cowork when you want the assistant to install
the release candidate for you:

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

After editing the config, ask me to fully restart Claude Desktop/Cowork, then
verify by calling anno_health and download_models from the MCP tools.
```

## Verification

After restart, ask Claude or Cowork:

```text
Call the anno_health tool and summarize the Hacienda version, build target,
available tools, and vault status.
```

If `anno_health` is not visible, the MCP server did not start or the client did
not reload the config. Model readiness is verified separately by calling
`download_models` or by running the first ingest/search workflow.

## Next Step

Create a first local corpus and search it:
[First Index And Search](first-index.md).
