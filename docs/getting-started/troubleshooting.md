# Troubleshooting

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin
Language: Bilingual

Use this page for first-install issues with the release binary, Claude
Desktop/Cowork MCP setup, model downloads, vault unlock, and first search.

Utilisez cette page pour diagnostiquer les problemes d'installation, de
configuration MCP, de modeles, de vault et de recherche initiale.

## MCP Tools Not Visible

Symptoms:

- Claude Desktop or Cowork does not show `anno-rag` tools.
- `anno_health` is missing.
- The MCP client says the server failed to start.

Checks:

- Restart Claude Desktop or Cowork fully after editing config.
- Verify the `command` is an absolute path to `anno-rag` or `anno-rag.exe`.
- Verify `args` is exactly `["mcp"]`.
- Run the binary manually:

```powershell
anno-rag mcp
```

The command should start and wait on stdio. Stop it with Ctrl+C when testing
manually.

## anno_health Fails

Checks:

- Confirm the release binary matches your OS and CPU architecture.
- Confirm the config did not point to an old debug binary.
- Confirm the MCP client process has the same environment variables you expect
  from your terminal.
- Check whether the vault can initialize through the configured key path.
- Inspect client logs for process startup, JSON config, or keyring errors.

From a terminal:

```powershell
anno-rag vault status
```

Then restart the MCP client and call `anno_health` again. `anno_health` reports
engine, vault, and tool metadata; it does not validate model files. Use
`download_models` only when a model download or later ingest/search command
reports missing models.

## Model Download Fails

Checks:

- Confirm network access to the model hosts used by the downloader.
- Confirm there is enough disk space for about 970 MiB of models.
- Retry with an explicit local directory:

```powershell
anno-rag download-models --dir "$HOME\.anno-rag\models"
$env:ANNO_MODELS_DIR = "$HOME\.anno-rag\models"
```

If the MCP client still reports missing models, add the same `ANNO_MODELS_DIR`
value to the MCP server `env` block.

## Vault Cannot Unlock

Checks:

- Prefer the OS keyring. Do not set `ANNO_RAG_VAULT_PASSPHRASE` unless your
  admin manages it explicitly.
- If you did set `ANNO_RAG_VAULT_PASSPHRASE`, make sure the exact same secret
  is available to the CLI and MCP server process.
- Do not log, paste, or share the passphrase.
- Run:

```powershell
anno-rag vault status
```

`vault status` checks the local keyring path. If your deployment intentionally
uses `ANNO_RAG_VAULT_PASSPHRASE`, a missing keyring entry does not by itself
mean the env-based vault path is broken. Confirm the same passphrase is present
for both CLI and MCP processes.

If the vault key changed, old encrypted mappings may no longer unlock. Treat
this as a data recovery or rotation event, not a normal search issue.

## Search Returns No Results

Checks:

- Confirm ingest completed:

```powershell
anno-rag ingest "$HOME\hacienda-sample\corpus" --output "$HOME\hacienda-sample\outputs"
```

- Confirm search syntax with:

```powershell
anno-rag search "confidentialite" --top-k 5
```

- Use a query term that appears in the sample document.
- Confirm the CLI and MCP server run as the same OS user.
- Confirm the MCP server uses the same data directory and model directory as
  the CLI session.

## Related Docs

- [Installation](installation.md)
- [Claude Desktop And Cowork Setup](claude-desktop-cowork.md)
- [Operations](../admins/operations.md)
- [File Layout](../reference/file-layout.md)
