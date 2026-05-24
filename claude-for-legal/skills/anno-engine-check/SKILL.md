---
name: anno-engine-check
description: >
  Verify the installed anno-rag engine satisfies this plugin's version and
  tool requirements before any other anno tool is called. Reads
  `engine-compat.json` from the plugin root, calls `anno_health` on the MCP
  server, and routes any drift into a one-line agent reply (notice or
  blocker). Invoked as the first step of every practice-area skill that
  touches anno tools.
argument-hint: ""
---

# /anno-engine-check

This skill is **not** a hook. It is invoked explicitly by practice-area
skills as their first step. Pattern matches `auto-updater`: explicit,
user-visible, audit-friendly.

## Steps

1. **Load `engine-compat.json`** from the active plugin's root directory.
   Required fields: `min_engine_version`, `recommended_engine_version`,
   `required_tools` (array), `release_page_url`.

   If the file is missing, treat as `min = recommended = "0.0.0"`,
   `required_tools = []`, `release_page_url = "(unknown)"`. Log a
   one-line debug notice but do not block.

2. **Call `anno_health`** on the connected anno MCP server. Parse the JSON
   into `{engine_version, build_target, signed, extension_install,
   vault_initialized, available_tools}`.

   If the MCP call fails, emit the **MCP unreachable blocker** below and abort.

3. **Semver-compare `engine_version` against `min_engine_version`** and
   `recommended_engine_version`. Use lexicographic semver (treat
   `pre-release` and `build-metadata` per semver 2.0.0).

4. **Compute `missing_tools = required_tools - available_tools`** (set
   difference, exact string match).

5. **Route into one of these outcomes (first match wins):**

   | Condition | Reply |
   |---|---|
   | `engine < min` | **Blocker.** "anno engine `{engine_version}` is below the minimum `{min_engine_version}` required by `{plugin}`. Download a newer engine from {release_page_url} and reinstall." → Abort caller. |
   | `missing_tools` not empty | **Blocker.** "anno engine `{engine_version}` is missing required tool(s): `{joined}`. Update the engine from {release_page_url}." → Abort caller. |
   | `engine_version` MCP call failed | **MCP unreachable blocker.** "anno engine is not running or not reachable. Open Claude Desktop → Settings → Connectors and verify `anno-rag` is enabled, or reinstall the Hacienda extension from {release_page_url}." → Abort caller. |
   | `min ≤ engine < recommended` | **Yellow notice.** "anno engine `{engine_version}` works for this skill but `{recommended_engine_version}` is available at {release_page_url}." → Proceed. |
   | `signed = false` AND `extension_install = false` AND we are NOT in a dev session | **Yellow notice.** "anno engine is a development build. Outputs are not suitable for production legal work." → Proceed. |
   | All checks pass | Silent. → Proceed. |

## Forward-compat contract

This skill MUST proceed silently when `engine > recommended` (engines are
backward-compatible across minor versions within a major per spec §13.4).
Never block on "engine too new."

## What this skill does NOT do

- It does **not** open the vault, ingest documents, or load models.
- It does **not** install or update the engine — only notifies.
- It does **not** retry on transient failures — the calling skill decides
  retry strategy.

## Runbook

See [docs/runbooks/anno-engine-check.md](../../docs/runbooks/anno-engine-check.md)
for diagnostic flows when this skill emits a blocker.
