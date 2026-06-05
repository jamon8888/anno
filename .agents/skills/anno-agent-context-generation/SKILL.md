---
name: anno-agent-context-generation
description: Use when updating concise Claude or Codex context from GitNexus, docs, Cargo metadata, and repo evidence.
---

# Anno Agent Context Generation

Keep always-on agent context short and evidence-backed.

1. Check GitNexus freshness:
   `npx gitnexus status`
2. Refresh only if stale:
   `npx gitnexus analyze`
3. Capture crate structure:
   `cargo metadata --format-version 1 --no-deps`
4. Capture command, MCP, release, and privacy evidence from existing docs:
   `rg -n "GitNexus|dev-fast|MCP|anno_health|vault|privacy|release" AGENTS.md docs README.md`
5. Run the dry-run context helper when present:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/agent-context-generate.ps1 -DryRun`
6. Update Claude/Codex context only with durable rules, package maps, fast-loop commands, privacy guidance, and verification expectations.
7. Move long procedures to skills, docs, runbooks, or plans instead of expanding always-on context.
