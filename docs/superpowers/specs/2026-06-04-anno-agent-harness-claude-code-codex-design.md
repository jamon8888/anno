# Anno Developer Agent Harness for Claude Code and Codex

**Date:** 2026-06-04
**Status:** Design accepted for planning
**Scope:** Repo-local developer harness for Claude Code and Codex. This design
covers instructions, hooks, agents, skills, setup scripts, and validation for
agent-assisted development in this repository. It does not change Hacienda
runtime behavior, MCP tool behavior, model inference, release packaging, or the
product install flow.

## 1. Goal

Provide a full developer setup for Claude Code that remains compatible with
Codex and raises the engineering floor for work in this Rust workspace.

The harness should:

1. Enforce non-negotiable safety rules through deterministic hooks.
2. Keep always-on instructions short and move larger workflows into skills.
3. Provide Claude Code subagents and Codex custom agents for focused review,
   security, build repair, and exploration work.
4. Use the repo's existing fast Rust loop instead of broad builds.
5. Preserve GitNexus-first code navigation and impact analysis.
6. Avoid leaking secrets, prompts, transcripts, vault data, or legal matter data.
7. Be installable and auditable from the repo with a dry-run mode.

## 2. Current Facts

- The repository already has a rich root `AGENTS.md` with GitNexus rules,
  Rust development loop guidance, and Codex-specific project context.
- The repository already has a root `CLAUDE.md`.
- `.claude/settings.json` exists and currently contains one inline
  `PostToolUse` hook that calls `scripts/dev-fast.ps1` after Rust edits.
- `.claude/skills/gitnexus/*` exists.
- No project-local `.codex/` directory is currently present.
- `scripts/hooks/pre-commit`, `scripts/hooks/pre-push`, and
  `scripts/hooks/commit-msg` already cover Git hooks.
- `scripts/dev-fast.ps1` is the preferred local Rust check loop.
- GitNexus is indexed and current for commit `7e34027`.
- The imported `harness.zip` is a generic Claude Code Rust harness with Bash
  hooks, agents, and skills. It is useful as inspiration, but it is not tailored
  to this Windows-first, privacy-heavy, GitNexus-indexed repo.

## 3. Official Tooling Baseline

This design follows current official documentation checked on 2026-06-04:

- Claude Code extension guidance distinguishes `CLAUDE.md`, rules, skills,
  subagents, MCP, hooks, and plugins by context cost and purpose. It explicitly
  recommends hooks for rules that must be guaranteed.
  Source: https://code.claude.com/docs/en/features-overview
- Claude Code hooks can block lifecycle events such as `PreToolUse`, `Stop`,
  `SubagentStop`, and `UserPromptSubmit`; command hooks use exit code `2` for
  blocking enforcement.
  Source: https://code.claude.com/docs/en/hooks
- Claude Code plugins can package skills, agents, hooks, and MCP servers, while
  standalone `.claude/` configuration is appropriate for project-specific
  customization and experimentation.
  Source: https://code.claude.com/docs/en/plugins
- Codex reads `AGENTS.md` instructions, supports project `.codex/config.toml`
  and `.codex/hooks.json` in trusted repos, and supports MCP, skills, and
  custom agents.
  Sources:
  https://developers.openai.com/codex/guides/agents-md
  https://developers.openai.com/codex/config-basic
  https://developers.openai.com/codex/hooks
  https://developers.openai.com/codex/skills
  https://developers.openai.com/codex/subagents

## 4. Non-Goals

- Do not replace the existing Git hooks.
- Do not install global user configuration silently.
- Do not write secrets, API keys, vault passphrases, or tokens.
- Do not upload telemetry or transcripts.
- Do not make broad `cargo build --workspace`, release, or all-feature builds
  the default stop condition.
- Do not package the harness as a marketplace plugin in the first phase.
- Do not alter Hacienda's product MCP install flow; that belongs to the
  separate cross-platform MCP setup track.

## 5. Recommended Architecture

Use a hybrid repo-local setup that is structured like a future plugin but ships
first as normal project files:

```text
AGENTS.md                         # Codex and cross-agent source of truth
CLAUDE.md                         # Claude Code short always-on entrypoint
.claude/
  settings.json                   # Claude Code hooks and project settings
  agents/
    anno-rust-reviewer.md
    anno-security-reviewer.md
    anno-build-resolver.md
    anno-doc-writer.md
    anno-gitnexus-explorer.md
    anno-release-gate.md
  rules/
    rust.md
    privacy.md
    gitnexus.md
  skills/
    anno-fast-debug-loop/SKILL.md
    anno-gitnexus-impact/SKILL.md
    anno-security-review/SKILL.md
    anno-mcp-smoke/SKILL.md
    anno-release-check/SKILL.md
.codex/
  config.toml
  hooks.json
  agents/
    explorer.toml
    reviewer.toml
    security.toml
    build-fixer.toml
.agents/
  skills/
    anno-fast-debug-loop/SKILL.md
    anno-gitnexus-impact/SKILL.md
    anno-security-review/SKILL.md
    anno-mcp-smoke/SKILL.md
    anno-release-check/SKILL.md
scripts/
  agent-harness/
    setup-agent-harness.ps1
    setup-agent-harness.sh
    block-dangerous-tool.ps1
    block-dangerous-tool.sh
    post-edit-rust-check.ps1
    post-edit-rust-check.sh
    stop-verify.ps1
    stop-verify.sh
    pre-compact-summary.ps1
    pre-compact-summary.sh
    prompt-secret-scan.ps1
    prompt-secret-scan.sh
    harness-status.ps1
```

PowerShell is the primary implementation path because the local repo guidance
and fast Rust loop are Windows-first. Bash scripts remain compatibility shims
for WSL, macOS, and Linux.

## 6. Instruction Layer

### 6.1 `AGENTS.md`

Keep the existing root `AGENTS.md` as the durable Codex and cross-agent
instruction file. Do not expand it with large reference material. Future edits
should keep it focused on:

- GitNexus obligations.
- Fast Rust loop.
- Safety and privacy rules.
- Where to find skills and plans.
- What verification evidence is expected before completion.

Codex discovers `AGENTS.md` at project startup and may combine nested
instructions, so large always-on content should be avoided.

### 6.2 `CLAUDE.md`

Reshape `CLAUDE.md` into the Claude Code equivalent of the root project
brief. It should be shorter than the current all-in-one style and should point
to skills and rules for details. Claude Code docs recommend keeping always-on
project memory concise and moving larger task workflows to skills or rules.

`CLAUDE.md` should contain:

- Project identity and package map.
- "Use GitNexus before code edits" rule.
- Fast Rust check loop.
- Privacy and secret handling.
- Commit and review expectations.
- Pointers to `.claude/rules/*` and `.claude/skills/*`.

### 6.3 Rules

Use `.claude/rules/` for narrower guidance:

- `rust.md`: Rust style, `unwrap` handling, async, tracing, error types.
- `privacy.md`: no cleartext legal matter data in logs, no vault secrets, no
  prompt transcript persistence by default.
- `gitnexus.md`: impact analysis, stale index handling, detect-change fallback.

Rules let Claude Code load path- or topic-scoped instructions without growing
the main `CLAUDE.md`.

## 7. Hook Layer

Hooks are the enforcement layer. Prompt instructions are advisory; hooks are
where safety rules become deterministic.

### 7.1 Shared Design Rules

Every hook script should:

- Read JSON from stdin.
- Validate input shape defensively.
- Resolve paths relative to the hook `cwd`.
- Fail closed only for clear policy violations.
- Use exit code `2` only when the action must be blocked.
- Use exit code `1` only for non-blocking hook errors.
- Avoid printing secrets or full prompt content.
- Write logs only to ignored local directories.
- Support `--dry-run` or test fixture execution where practical.

### 7.2 `PreToolUse` / Dangerous Tool Blocker

Purpose: prevent irreversible or high-risk operations before they run.

Block patterns:

- Recursive deletion of root, home, repo root, `.git`, `.codex`, `.claude`, or
  broad wildcard targets.
- `git reset --hard`, `git clean -fdx`, `git checkout --`, and equivalent
  destructive commands unless the user explicitly requested that operation in
  the current task and the tool request includes a matching explanation.
- Writes to `.env`, key files, vault files, local model credentials, or
  configured secret paths.
- Commands that echo or commit likely secrets.
- Broad Rust builds such as `cargo build --workspace`, `cargo build --release`,
  or full all-feature builds during normal debugging, unless an override env var
  is set or the user explicitly requested release validation.
- Shell chaining that enumerates paths and then deletes or moves them through a
  different shell.

Allow normal targeted commands:

- `scripts/dev-fast.ps1`.
- Targeted `cargo check -p <crate>`.
- Targeted tests.
- `git status`, `git diff`, `git log`, `rg`, and read-only exploration.

Claude Code implementation:

- Configure in `.claude/settings.json` as `PreToolUse` for `Bash`, `Edit`,
  `Write`, and relevant MCP tool names.
- Use command scripts in `scripts/agent-harness/`.

Codex implementation:

- Configure in `.codex/hooks.json` as `PreToolUse` and `PermissionRequest`.
- Use the same scripts where input schemas align, with a thin adapter if needed.

### 7.3 `UserPromptSubmit` / Secret Scan

Purpose: stop accidental prompt submission of obvious credentials.

The scanner should detect high-confidence secret patterns only:

- API key formats.
- Private key blocks.
- Bearer tokens.
- `.env`-style password assignments.

It should not block ordinary legal text, user names, email addresses, or client
matter facts, because this repository intentionally works with PII locally. The
hook must report only the category and rough location, not the secret itself.

### 7.4 `PostToolUse` / Rust Edit Check

Purpose: after file edits, run the cheapest useful verification and feed results
back to the agent.

Behavior:

1. Detect changed files from hook input and `git diff --name-only`.
2. If a `.rs` file changed, run `rustfmt` on that file.
3. Detect the crate from `crates/<crate>/...`.
4. Run:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check
   ```

5. Write a local verification stamp under an ignored directory such as
   `.agent-harness/state/last-check.json`.
6. If the command fails, return additional context explaining the failing check
   and the targeted command to rerun.

Do not run broad clippy or workspace tests in this hook. That belongs to
explicit review, stop verification, pre-push, or release gates.

### 7.5 `Stop` / Turn Completion Gate

Purpose: prevent an agent from claiming completion when basic verification is
missing.

Rules:

- Docs-only changes: allow stop after a self-review check.
- Rust changes: require either a successful recent targeted `dev-fast` stamp or
  a clear final explanation that verification was impossible.
- Security-sensitive changes: require security review skill or agent evidence.
- Generated binary/model/output files: block stop unless the final message
  explains why they exist and whether they should be committed.
- Existing unrelated dirty files must not block the agent, but the hook should
  distinguish pre-existing changes from new files touched in the current turn
  when possible.

For Claude Code, `Stop` can continue the conversation by returning a blocking
reason. For Codex, `Stop` should provide developer context or block based on the
Codex hook schema.

### 7.6 `SubagentStop`

Purpose: keep specialized reviewers honest without forcing the main agent to
ingest all intermediate logs.

Rules:

- Reviewer subagents must return findings first, ordered by severity, with file
  references.
- Build resolver subagents must include the command that failed, root cause, and
  verification command.
- Security subagents must explicitly state whether secrets, auth, filesystem
  paths, network IO, crypto, and unsafe Rust were in scope.

### 7.7 `PreCompact`

Purpose: preserve continuity without recording sensitive transcripts by default.

Default behavior:

- Write a local compact summary with timestamp, branch, changed file list, and
  active verification state.
- Do not copy the full transcript by default.

Opt-in behavior:

- If `ANNO_AGENT_HARNESS_BACKUP_TRANSCRIPTS=1`, copy the full transcript into an
  ignored local directory.

This respects Hacienda's privacy posture while still helping long sessions.

## 8. Agents and Subagents

### 8.1 Claude Code Agents

Define repo-local agents in `.claude/agents/`:

- `anno-rust-reviewer`: read-only Rust review. Focuses on correctness,
  `unwrap`, async blocking, error handling, tests, and performance risks.
- `anno-security-reviewer`: read-only security review. Focuses on secrets,
  auth, path traversal, command execution, vault handling, network IO, and
  unsafe blocks.
- `anno-build-resolver`: can edit. Uses systematic build debugging and targeted
  `dev-fast` commands.
- `anno-doc-writer`: can edit docs and public API comments. Runs doc-specific
  validation when practical.
- `anno-gitnexus-explorer`: read-only. Uses GitNexus before file reads when
  exploring unfamiliar flows.
- `anno-release-gate`: read-only unless explicitly asked. Checks release
  readiness, generated artifacts, docs, and local gate status.

Agents should include tool restrictions. Reviewers should be read-only.
Resolvers may edit only when explicitly delegated.

### 8.2 Codex Agents

Define equivalent project-scoped custom agents in `.codex/agents/`:

- `explorer.toml`: read-heavy codebase exploration.
- `reviewer.toml`: code quality and correctness review.
- `security.toml`: security diff or scoped audit.
- `build-fixer.toml`: build/test error investigation and minimal fixes.

Codex subagents are enabled by default in current releases and only spawn when
explicitly requested, so `AGENTS.md` should teach when to ask for them without
making every task parallel.

## 9. Skills

Use skills for workflows and reference material that should not be always-on.

Shared skill set:

- `anno-fast-debug-loop`: how to choose `dev-fast`, targeted tests, and
  nextest profiles.
- `anno-gitnexus-impact`: how to run query, context, impact, and stale-index
  recovery.
- `anno-security-review`: security checklist tailored to Hacienda's vault,
  gateway, MCP, and local legal-data workflows.
- `anno-mcp-smoke`: how to smoke test `anno-rag mcp`, `tools/list`, and
  `anno_health`.
- `anno-release-check`: release/package verification and broad gate escalation.

Store Codex-readable skills under `.agents/skills/`. For Claude Code, mirror
the same core workflows under `.claude/skills/` or package them through a future
plugin. Keep descriptions concise so they trigger accurately without crowding
context.

## 10. Setup Script

Add a repo-local installer:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -Target all
```

Unix fallback:

```bash
scripts/agent-harness/setup-agent-harness.sh --dry-run
scripts/agent-harness/setup-agent-harness.sh --target all
```

Targets:

- `claude-code`: writes or merges `.claude/settings.json`, agents, rules, and
  skills.
- `codex`: writes or merges `.codex/config.toml`, `.codex/hooks.json`, agents,
  and shared skills.
- `git-hooks`: verifies existing `scripts/hooks/*` and optionally runs
  `just setup-hooks`.
- `mcp`: verifies GitNexus MCP availability for Claude Code and Codex, but does
  not write third-party credentials.
- `all`: all of the above.

Setup rules:

- Always create backups before modifying existing config files.
- Preserve existing settings and permissions.
- Do not modify `.claude/settings.local.json`.
- Do not change global `~/.claude` or `~/.codex` config unless the user passes
  an explicit `-Global` flag.
- Print exact files changed.
- Support `-DryRun`.
- Fail with clear instructions if required tools are missing.

## 11. MCP Baseline

The developer harness should make MCP availability explicit, not magic.

Required for this repo:

- GitNexus local MCP or CLI access for code intelligence.

Recommended but optional:

- Documentation lookup MCP for up-to-date external docs.
- GitHub MCP for PR and issue workflows.
- Playwright MCP for frontend/browser verification when relevant.

The setup should detect and report missing MCPs. It may print commands to add
them, but should not install remote MCPs or write secrets automatically.

## 12. Privacy and Security

This repository handles legal and PII-sensitive workflows. The harness must:

- Never log full prompts by default.
- Never store full transcripts by default.
- Never copy vault files, model cache secrets, or local passphrases.
- Redact likely secrets in hook output.
- Keep local harness state under ignored paths.
- Treat user-provided legal content as potentially sensitive even when local.
- Use path canonicalization before allowing destructive operations.
- Deny writes to secret-bearing files unless explicitly requested and reviewed.

## 13. Verification Plan

Implementation should include fixture-based tests for hook scripts.

Minimum tests:

- Dangerous command blocker denies root/home/repo recursive deletion.
- Dangerous command blocker allows read-only `git status`, `rg`, and targeted
  `dev-fast` commands.
- Secret prompt scanner blocks a private key block and redacts the output.
- Secret prompt scanner does not block ordinary PII/legal text.
- Rust edit checker maps `crates/anno-rag/src/foo.rs` to package `anno-rag`.
- Stop gate allows docs-only changes.
- Stop gate blocks Rust changes without a verification stamp.
- Setup dry-run reports intended changes without writing files.
- Setup merge preserves existing `.claude/settings.json` permissions and hooks.
- Codex hooks JSON validates as JSON.
- Claude settings JSON validates as JSON.

Manual smoke:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -PrintOnly
```

If Rust code is edited during implementation:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check
```

Before committing implementation, run GitNexus change detection when available.
If only the CLI is available and it lacks `detect_changes`, run `npx gitnexus
status`, inspect `git diff --name-status`, and document the fallback.

## 14. Rollout Phases

### Phase 1: Spec and inventory

- Commit this design.
- Inventory existing `.claude` settings, Git hooks, and `.agents/skills`.
- Confirm which files are pre-existing dirty work and should not be touched.

### Phase 2: Shared hook scripts

- Add `scripts/agent-harness/`.
- Implement PowerShell scripts first.
- Add Bash compatibility shims.
- Add fixture tests for JSON hook inputs.

### Phase 3: Claude Code layer

- Replace the inline `.claude/settings.json` hook with script-backed hooks.
- Add Claude Code agents, rules, and skills.
- Validate JSON and run dry-run setup.

### Phase 4: Codex layer

- Add `.codex/config.toml`, `.codex/hooks.json`, and custom agents.
- Mirror shared skills under `.agents/skills/`.
- Validate Codex hook schemas and trust instructions.

### Phase 5: Setup and status commands

- Add setup and status scripts.
- Document usage in `docs/developers/configuration.md` or
  `docs/README.md`.
- Keep all global/user modifications opt-in.

### Phase 6: Future plugin packaging

- Convert the stable Claude Code portion into a plugin only after repo-local
  behavior has been validated.
- Preserve the Codex layer as project config because Codex plugin packaging is
  separate from Claude Code plugin packaging.

## 15. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Hooks become too aggressive and block normal work | Start with high-confidence denies, support dry-run, and allow explicit env overrides for broad gates. |
| Context bloat from too many instructions | Keep `CLAUDE.md` and `AGENTS.md` short; move workflows to skills and rules. |
| Windows/PowerShell and Bash behavior diverge | Make PowerShell primary and Bash a thin equivalent; test both with fixtures. |
| Existing dirty worktree gets mixed into commits | Stage only harness files and document dirty pre-existing files. |
| Transcript backup leaks sensitive data | Default to summaries only; full transcript backup requires explicit opt-in env var. |
| Codex and Claude hook schemas drift | Keep shared logic in scripts and tool-specific adapters minimal. |
| GitNexus CLI lacks `detect_changes` | Use MCP tool when available; otherwise document `status` plus `git diff --name-status` fallback. |

## 16. Acceptance Criteria

- Claude Code can load project settings without JSON errors.
- Codex can load project config and hooks in a trusted project.
- Dangerous command hooks block high-confidence destructive operations.
- Rust post-edit hook runs targeted formatting/checks rather than broad builds.
- Stop gate prevents unverified Rust completion claims while allowing docs-only
  work.
- Review/security/build agents exist with clear tool permissions.
- Skills exist for fast loop, GitNexus, security, MCP smoke, and release checks.
- Setup script supports dry-run and preserves existing config.
- No secret, transcript, vault, or model-cache data is committed.
- Implementation commits stage only expected harness files.
