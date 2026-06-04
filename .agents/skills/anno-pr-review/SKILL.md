---
name: anno-pr-review
description: Use when reviewing an Anno PR or local branch diff for correctness, docs, tests, crate, CLI, and MCP impact.
---

# Anno PR Review

Review findings first; summarize after findings.

1. Identify the base:
   `git merge-base HEAD origin/main`
2. Inspect scope:
   `git diff --name-status <base>...HEAD`
   `git diff --stat <base>...HEAD`
3. Read the diff for owned areas:
   `git diff <base>...HEAD -- crates docs scripts .codex .agents`
4. Use GitNexus impact for changed public symbols when available.
5. Check docs, tests, crate dependencies, CLI parity, MCP parity, release impact, and security-sensitive surfaces.
6. Generate helper output when the script is present:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/pr-review-generate.ps1 -DryRun`
7. Return findings with severity and file references, then open questions, then a short PR summary and test plan.
