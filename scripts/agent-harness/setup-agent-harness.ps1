param(
    [ValidateSet("all", "claude-code", "codex", "git-hooks", "mcp", "automation")]
    [string]$Target = "all",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$RepoRoot = (git rev-parse --show-toplevel).Trim()

Write-Output "Anno agent harness setup"
Write-Output ("target: {0}" -f $Target)
Write-Output ("repo: {0}" -f $RepoRoot)

$paths = @(
    ".claude/settings.json",
    ".codex/config.toml",
    ".codex/hooks.json",
    ".agents/skills/anno-fast-debug-loop/SKILL.md",
    "scripts/agent-harness/block-dangerous-tool.ps1",
    "scripts/agent-harness/changelog-generate.ps1"
)

foreach ($relativePath in $paths) {
    $fullPath = Join-Path $RepoRoot $relativePath
    if (Test-Path -LiteralPath $fullPath) {
        Write-Output ("ok: {0}" -f $relativePath)
    } else {
        Write-Output ("missing: {0}" -f $relativePath)
    }
}

if ($DryRun) {
    Write-Output "dry-run: no files written"
    exit 0
}

Write-Output "setup verified existing repo-local harness files"
