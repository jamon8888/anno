---
name: anno-changelog
description: Use when drafting or validating Anno changelog entries from commits and diffs.
---

# Anno Changelog

Generate changelog notes from evidence, not memory.

1. Inspect commits since the base:
   `git log --oneline --decorate <base>..HEAD`
2. Inspect changed files:
   `git diff --name-status <base>...HEAD`
3. Generate the dry-run changelog when the script is present:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/changelog-generate.ps1 -DryRun`
4. Group entries by user-facing behavior, CLI, MCP, crates, docs, tests, release packaging, and internal maintenance.
5. Call out breaking changes, migration notes, known limitations, and omitted verification.
6. Keep final text concise and suitable for `CHANGELOG.md` or PR release notes.
