---
name: anno-doc-generation
description: Use when refreshing Anno command, MCP, crate, release, or agent context documentation from captured evidence.
---

# Anno Doc Generation

Refresh docs from commands and source evidence.

1. Capture changed scope:
   `git diff --name-status <base>...HEAD`
2. Capture command and MCP evidence from source and docs:
   `rg -n "anno_health|tools/list|setup-mcp|download-models|mcp" crates docs scripts README.md`
3. Capture crate evidence:
   `cargo metadata --format-version 1 --no-deps`
4. Run the dry-run doc generator when present:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/docs-generate.ps1 -DryRun`
5. Update existing docs locations first: `README.md`, `docs/developers/cli.md`, `docs/developers/mcp-tools.md`, `docs/release/README-release.md`, and relevant runbooks.
6. Keep generated agent context concise; move long evidence to existing docs or reports.
7. Re-run doc searches after edits to catch stale command names or old release tags.
