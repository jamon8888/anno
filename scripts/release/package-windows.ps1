[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Tag,

    [Parameter(Mandatory = $false)]
    [ValidateNotNullOrEmpty()]
    [string]$Target = "x86_64-pc-windows-msvc"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptPath = $PSCommandPath
if (-not $ScriptPath) {
    $ScriptPath = $MyInvocation.MyCommand.Path
}

$ReleaseDir = Split-Path -Parent $ScriptPath
$ScriptsDir = Split-Path -Parent $ReleaseDir
$RepoRoot = Split-Path -Parent $ScriptsDir

$PackageName = "hacienda-$Tag-$Target"
$DistDir = Join-Path -Path $RepoRoot -ChildPath "dist"
$StagingDir = Join-Path -Path $DistDir -ChildPath $PackageName
$ZipPath = Join-Path -Path $DistDir -ChildPath "$PackageName.zip"

$RequiredFiles = @(
    "target/$Target/release/anno-rag.exe",
    "target/$Target/release/anno-privacy-gateway.exe",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "env.example",
    "docs/release/examples/claude_desktop_config.windows.json",
    "docs/release/examples/claude_desktop_config.macos.json"
)

$MissingFiles = foreach ($RelativePath in $RequiredFiles) {
    $FullPath = Join-Path -Path $RepoRoot -ChildPath $RelativePath
    if (-not (Test-Path -LiteralPath $FullPath -PathType Leaf)) {
        $RelativePath
    }
}

if ($MissingFiles.Count -gt 0) {
    $MissingList = $MissingFiles -join [Environment]::NewLine
    throw "Cannot create Windows package. Missing required file(s):$([Environment]::NewLine)$MissingList"
}

New-Item -ItemType Directory -Path $DistDir -Force | Out-Null

if (Test-Path -LiteralPath $StagingDir) {
    Remove-Item -LiteralPath $StagingDir -Recurse -Force
}

if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}

New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null

foreach ($RelativePath in $RequiredFiles) {
    $SourcePath = Join-Path -Path $RepoRoot -ChildPath $RelativePath
    Copy-Item -LiteralPath $SourcePath -Destination $StagingDir
}

$StagedFiles = Get-ChildItem -LiteralPath $StagingDir -File
Compress-Archive -LiteralPath $StagedFiles.FullName -DestinationPath $ZipPath -Force

Write-Output $ZipPath
