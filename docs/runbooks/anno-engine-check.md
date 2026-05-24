# anno-engine-check runbook

Diagnostic flows for the `claude-for-legal/skills/anno-engine-check`
skill (spec §13.3).

## Triage table

| Blocker text the user saw | Likely cause | Next step |
|---|---|---|
| "anno engine `X.Y.Z` is below the minimum…" | User upgraded the plugin but not the engine. | Download the engine artifact for their OS from the release page and reinstall. |
| "anno engine is missing required tool(s): …" | Plugin expects a tool the engine does not yet expose. Usually a plugin-too-new condition. | Same: upgrade the engine. Confirm `anno_health.available_tools` after the upgrade. |
| "anno engine is not running or not reachable." | (a) Claude Desktop has not loaded the connector; (b) `anno-rag` binary missing or quarantined by AV; (c) `claude_desktop_config.json` entry corrupt. | Open Claude Desktop → Settings → Connectors. Confirm `anno-rag` is enabled and shows healthy. If absent, reinstall the Hacienda extension. If AV quarantined, ask IT to whitelist the engine path. |
| "anno engine is a development build…" | User installed a non-CI build. | Replace with a release build from the official release page. Acceptable to ignore for dev environments. |

## Verifying engine health manually

From a terminal:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"anno_health","arguments":{}}}' | anno-rag mcp
```

The response JSON should contain `engine_version`, `available_tools`, and
`vault_initialized`. Compare against `claude-for-legal/engine-compat.json`.

## Verifying via CLI

```bash
anno-rag vault status
```

Reports keyring entry presence. Output does not include the key itself.

## When the plugin says "engine too old" but `anno-rag --version` says it is current

Two engines can be installed: one via the `.mcpb` extension (under
Claude Desktop's extension dir) and one manually on PATH. Claude Desktop
launches the extension copy, not the PATH copy. Check the extension's
installed version under Claude Desktop → Settings → Extensions.

## Escalation

If `anno_health` returns successfully but reports a version older than
the current release on GitHub, file an issue with:

- The full `anno_health` JSON output.
- The path Claude Desktop used to launch the engine (from
  `claude_desktop_config.json` mcpServers.anno-rag.command).
- The output of `anno-rag --version` from PATH.
- OS and Claude Desktop version.
