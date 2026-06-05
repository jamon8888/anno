---
name: anno-mcp-smoke
description: Use when validating Anno MCP startup, tool listing, or anno_health behavior.
---

# Anno MCP Smoke

Use the lightest MCP smoke that proves initialize, tools/list, and `anno_health`.

1. If a release or installed binary exists, run:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/mcp-full-smoke.ps1`
2. Override binary and model paths when needed:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/mcp-full-smoke.ps1 -Exe <path> -ModelsDir <path>`
3. For direct protocol debugging, use:
   `python scripts/mcp_full_smoke.py`
4. Confirm the smoke covers MCP `initialize`, `tools/list`, and `anno_health`.
5. If startup fails, capture the binary path, `ANNO_MODELS_DIR`, stderr, MCP log path, and whether the process is debug or release.
6. Do not add `ANNO_RAG_VAULT_PASSPHRASE` unless the user explicitly provides it.
