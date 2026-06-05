param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Write-Output "## Agent Context Generation Plan"
Write-Output "- Review AGENTS.md for durable Codex rules and project-level expectations."
Write-Output "- Review CLAUDE.md for Claude-specific context that should remain aligned."
Write-Output "- Audit .claude/rules for always-on and language-specific guidance."
Write-Output "- Audit .agents/skills for workflow-specific guidance that should not be duplicated in always-on context."
Write-Output "- Refresh docs/developers/agent-context.md with concise evidence-backed context."

if ($DryRun) {
    Write-Output "dry-run: no files written"
}
