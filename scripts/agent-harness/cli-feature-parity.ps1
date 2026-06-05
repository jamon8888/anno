param(
    [string]$DiffNameStatusPath = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Get-DiffNameStatusLines {
    param(
        [string]$Path
    )

    if (-not [string]::IsNullOrWhiteSpace($Path)) {
        return @(Get-Content -LiteralPath $Path)
    }

    return @(git diff --name-status HEAD)
}

function Get-ChangedPathFromNameStatus {
    param(
        [string]$Line
    )

    if ([string]::IsNullOrWhiteSpace($Line)) {
        return ""
    }

    $parts = $Line -split "`t"
    if ($parts.Count -lt 2) {
        return ""
    }

    return ([string]$parts[$parts.Count - 1]).Replace("\", "/")
}

function Test-AnnoRagSurfacePath {
    param(
        [string]$Path
    )

    return ($Path -like "crates/anno-rag/*" -or
        $Path -like "crates/anno-rag-mcp/*" -or
        $Path -like "crates/anno-rag-tabular/*")
}

$paths = New-Object System.Collections.Generic.List[string]
foreach ($line in Get-DiffNameStatusLines -Path $DiffNameStatusPath) {
    $path = Get-ChangedPathFromNameStatus -Line $line
    if (-not [string]::IsNullOrWhiteSpace($path)) {
        $paths.Add($path)
    }
}

$annoRagPaths = @($paths | Where-Object { Test-AnnoRagSurfacePath -Path $_ } | Sort-Object -Unique)
$cliPaths = @($paths | Where-Object { $_ -like "crates/anno-rag-bin/*" } | Sort-Object -Unique)
$docsPaths = @($paths | Where-Object { $_ -eq "README.md" -or $_ -like "docs/*" } | Sort-Object -Unique)

if ($annoRagPaths.Count -eq 0) {
    Write-Output "no anno-rag surface changes detected"
} else {
    Write-Output ("anno-rag change detected: {0}" -f ($annoRagPaths -join ", "))

    if ($cliPaths.Count -gt 0) {
        Write-Output ("anno-rag-bin touched: {0}" -f ($cliPaths -join ", "))
    } else {
        Write-Output "warning: anno-rag-bin not touched"
    }

    if ($docsPaths.Count -gt 0) {
        Write-Output ("docs touched: {0}" -f ($docsPaths -join ", "))
    } else {
        Write-Output "warning: docs not touched"
    }
}

if ($DryRun) {
    Write-Output "dry-run: no files written"
}
