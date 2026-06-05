param(
    [string]$Base = "main",
    [string]$DiffNameStatusPath = ""
)

$ErrorActionPreference = "Stop"

if (-not [string]::IsNullOrWhiteSpace($DiffNameStatusPath)) {
    $nameStatusLines = @(Get-Content -LiteralPath $DiffNameStatusPath)
} else {
    $nameStatusLines = @(git diff --name-status "$Base...HEAD")
}

$paths = New-Object System.Collections.Generic.List[string]
foreach ($line in $nameStatusLines) {
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }

    $parts = ([string]$line) -split "`t"
    if ($parts.Count -lt 2) {
        continue
    }

    $path = ([string]$parts[$parts.Count - 1]).Replace("\", "/")
    if (-not [string]::IsNullOrWhiteSpace($path)) {
        $paths.Add($path)
    }
}

$uniquePaths = @($paths | Sort-Object -Unique)
$annoRagSurfacePaths = @($uniquePaths | Where-Object { $_ -like "crates/anno-rag/*" })

Write-Output "## Findings"
Write-Output "- No automatic critical findings from path-level review."
Write-Output "- Manual code review is still required."

Write-Output ""
Write-Output "## Changed Areas"
if ($uniquePaths.Count -eq 0) {
    Write-Output "- (none)"
} else {
    foreach ($path in $uniquePaths) {
        Write-Output ("- {0}" -f $path)
    }
}

Write-Output ""
Write-Output "## CLI and MCP Parity"
if ($annoRagSurfacePaths.Count -gt 0) {
    Write-Output ("- Warning: anno-rag surface path detected: {0}" -f ($annoRagSurfacePaths -join ", "))
    Write-Output "- Verify matching CLI, MCP, docs, and tests before merging."
} else {
    Write-Output "- No anno-rag surface path detected."
}

Write-Output ""
Write-Output "## Test Plan"
Write-Output "- Run the targeted agent harness tests."
Write-Output "- Run any changed crate or CLI/MCP checks required by the changed paths."
