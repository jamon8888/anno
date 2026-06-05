---
name: anno-crate-dependency-map
description: Use when mapping Anno crate dependencies, dependents, or changed-crate blast radius.
---

# Anno Crate Dependency Map

Use Cargo metadata before reasoning about crate impact.

1. Generate local metadata:
   `cargo metadata --format-version 1 --no-deps`
2. Generate a readable dry-run map when the script is present:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/crate-map-generate.ps1 -DryRun`
3. For changed files, identify packages:
   `git diff --name-only <base>...HEAD`
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1 -PrintOnly`
4. Check direct dependencies and reverse dependencies for touched crates.
5. For shared crates such as `anno`, `anno-rag`, and `anno-rag-mcp`, consider `-AllAffected` checks.
6. Report package impact, likely downstream tests, CLI/MCP docs impact, and release packaging impact.
