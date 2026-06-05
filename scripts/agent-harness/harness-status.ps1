$ErrorActionPreference = "Stop"

$RepoRoot = (git rev-parse --show-toplevel).Trim()

$checks = [ordered]@{
    "claude_settings" = ".claude/settings.json"
    "codex_config" = ".codex/config.toml"
    "codex_hooks" = ".codex/hooks.json"
    "shared_skills" = ".agents/skills/anno-fast-debug-loop/SKILL.md"
    "harness_scripts" = "scripts/agent-harness/block-dangerous-tool.ps1"
}

foreach ($key in $checks.Keys) {
    $relativePath = $checks[$key]
    $fullPath = Join-Path $RepoRoot $relativePath
    $exists = [bool](Test-Path -LiteralPath $fullPath)
    Write-Output ("{0}: {1}" -f $key, $exists)
}
