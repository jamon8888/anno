---
name: anno-cli-feature-parity
description: Use when Anno library, MCP, or tabular changes may require matching CLI, docs, examples, or smoke-test updates.
---

# Anno CLI Feature Parity

Run this workflow when user-facing `anno-rag`, `anno-rag-mcp`, or `anno-rag-tabular` behavior changes.

1. Inspect changed files:
   `git diff --name-status <base>...HEAD`
2. Use GitNexus context or impact for changed public symbols when available.
3. Check CLI command modules under `crates/anno-cli/src/cli/commands/` and `crates/anno-rag-bin/src/`.
4. Check docs and references: `README.md`, `docs/reference/commands.md`, `docs/developers/mcp-tools.md`, `docs/release/README-release.md`, and examples.
5. Run the dry-run parity helper when present:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/cli-feature-parity.ps1 -DryRun`
6. Classify drift as internal-only, warning-level, or high-confidence user-facing drift.
7. Recommend targeted tests or smokes for changed CLI and MCP surfaces.
