param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Write-Output "## Docs Generation Evidence Plan"
Write-Output "- Capture command help: anno-rag --help"
Write-Output "- Capture review command help: anno-rag review --help"
Write-Output "- Run cargo metadata: cargo metadata --format-version 1 --no-deps"
Write-Output "- Run a docs audit against README.md, docs/reference, docs/developers, and docs/release."
Write-Output "- Update existing documentation locations from captured evidence."

if ($DryRun) {
    Write-Output "dry-run: no files written"
}
