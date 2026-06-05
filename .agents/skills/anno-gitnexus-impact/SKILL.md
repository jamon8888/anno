---
name: anno-gitnexus-impact
description: Use before editing Anno code symbols or when assessing blast radius with GitNexus.
---

# Anno GitNexus Impact

Use GitNexus before code-symbol edits and when exploring unfamiliar flows.

1. Check freshness:
   `npx gitnexus status`
2. If stale, refresh before relying on graph results:
   `npx gitnexus analyze`
3. Find relevant flows:
   `gitnexus_query({query: "<feature, error, or concept>"})`
4. Inspect the target:
   `gitnexus_context({name: "<symbolName>"})`
5. Assess upstream blast radius before edits:
   `gitnexus_impact({target: "<symbolName>", direction: "upstream"})`
6. Warn before editing if impact is HIGH or CRITICAL.
7. When GitNexus MCP tools are unavailable, fall back to:
   `git diff --name-status`, `rg "<symbolName>" crates docs`, `cargo metadata --format-version 1`, and targeted tests.
8. Before commit, verify changed scope:
   `gitnexus_detect_changes({scope: "staged"})` when available, otherwise inspect `git diff --cached --name-status` and `git diff --cached`.
