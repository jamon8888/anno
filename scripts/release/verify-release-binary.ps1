[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$BinaryPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Resolved = Resolve-Path -LiteralPath $BinaryPath
$PathText = $Resolved.Path

if ($PathText -match "\\debug\\") {
    throw "Release verification rejected debug binary path: $PathText"
}

if ($PathText -notmatch "\\(release|dist)\\") {
    throw "Release verification expected a release or dist profile path, got: $PathText"
}

$Item = Get-Item -LiteralPath $PathText
if ($Item.Length -lt 1MB) {
    throw "Release binary is unexpectedly small: $($Item.Length) bytes"
}

$Output = & $PathText --help 2>&1
$ExitCode = $LASTEXITCODE

if ($ExitCode -ne 0) {
    throw "Binary --help failed with exit code $ExitCode. Output: $Output"
}

if (($Output -join "`n") -notmatch "mcp") {
    throw "Binary help output does not mention the mcp command"
}

Write-Output "verify-release-binary: OK $PathText"
