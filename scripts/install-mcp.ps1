# install-mcp.ps1 — Register anno-rag as an MCP server after extracting the archive.
#
# Usage (run from the directory containing this script and anno-rag.exe):
#   .\install-mcp.ps1
#   .\install-mcp.ps1 -DryRun        # Preview what would change
#   .\install-mcp.ps1 -SkipModels    # Skip model download (already done)
#
# Registers in:
#   Claude Desktop  → %APPDATA%\Claude\claude_desktop_config.json
#   Claude Code     → runs `claude mcp add` if the CLI is on PATH
#
# Restart Claude Desktop / Claude Code after running this script.
[CmdletBinding()]
param(
    [switch]$DryRun,
    [switch]$SkipModels
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $PSCommandPath
$Binary = Join-Path -Path $ScriptDir -ChildPath "anno-rag.exe"

if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    Write-Error "anno-rag.exe not found at $Binary. Run this script from the directory containing the anno-rag.exe binary."
    exit 1
}

$SetupArgs = @("setup-mcp", "--target", "all")
if ($SkipModels) { $SetupArgs += "--skip-models" }
if ($DryRun)     { $SetupArgs += "--dry-run" }

Write-Host "Registering anno-rag as MCP server..."
& $Binary @SetupArgs
if ($LASTEXITCODE -ne 0) {
    Write-Warning "MCP registration returned exit code $LASTEXITCODE. Check the output above."
    exit $LASTEXITCODE
}
Write-Host ""
Write-Host "Done. Restart Claude Desktop or Claude Code to load the server."
