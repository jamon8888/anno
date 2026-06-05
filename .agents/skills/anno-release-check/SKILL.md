---
name: anno-release-check
description: Use when validating Anno release readiness, packaging, changelog, docs, and MCP install behavior.
---

# Anno Release Check

Run release checks from the existing release scripts and docs.

1. Review release docs:
   `Get-Content docs/release/README-release.md`
   `Get-Content docs/admins/release-management.md`
2. Dry-run the local release gate:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release/local-pipeline-gate.ps1 -DryRun -SkipHeavy -SkipOcr -SkipMcp`
3. For binary validation, use:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release/verify-release-binary.ps1 -Exe <path>`
4. For MCPB validation, use:
   `python scripts/release/verify-mcpb.py <path>`
5. For packaging checks, inspect `scripts/release/package-windows.ps1`, `package-unix.sh`, accelerated package scripts, and checksum scripts.
6. Review changelog and PR summary with `anno-changelog`.
7. Confirm docs mention current tags, install commands, MCP config, model download behavior, gateway smoke, and known limitations.
8. Report release blockers, warnings, and exact commands that passed or were not run.
