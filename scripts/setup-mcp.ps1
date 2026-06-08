[CmdletBinding()]
param(
    [ValidateSet("desktop", "claude-code", "all", "manual")]
    [string]$Target = "all",

    [ValidateSet("release", "local-build", "path")]
    [string]$Source = "release",

    [string]$Tag = "latest",

    [string]$Binary,

    [string]$InstallDir = "$env:LOCALAPPDATA\anno-rag",

    [string]$ModelsDir = "$env:USERPROFILE\.anno-rag\models",

    [switch]$SkipModels,

    [switch]$DryRun,

    [switch]$Force,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$AllowedRoots = @()
for ($i = 0; $i -lt $RemainingArgs.Count; $i++) {
    $arg = $RemainingArgs[$i]
    if ($arg -in @("-AllowedRoot", "-allowed-root", "--allowed-root")) {
        if (($i + 1) -ge $RemainingArgs.Count -or $RemainingArgs[$i + 1].StartsWith("-")) {
            throw "$arg requires a value"
        }
        $AllowedRoots += $RemainingArgs[$i + 1]
        $i++
        continue
    }
    throw "unknown argument: $arg"
}

function Get-ReleaseTag {
    param([string]$RequestedTag)

    if ($RequestedTag -ne "latest") {
        return $RequestedTag
    }

    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/jamon8888/anno/releases/latest"
    return [string]$release.tag_name
}

function Resolve-LocalBuildBinary {
    $repoRoot = (& git rev-parse --show-toplevel).Trim()
    $null = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path -Path $repoRoot -ChildPath "scripts\mcp-iterate.ps1") -Mode install -SkipCheck -InstallDir $InstallDir
    if ($LASTEXITCODE -ne 0) {
        throw "local build install failed with exit code $LASTEXITCODE"
    }

    $candidate = Join-Path -Path $InstallDir -ChildPath "anno-rag.exe"
    return (Resolve-Path -LiteralPath $candidate).Path
}

function Install-ReleaseBinary {
    param([string]$ResolvedTag)

    $target = "x86_64-pc-windows-msvc"
    $asset = "hacienda-$ResolvedTag-$target.zip"
    $base = "https://github.com/jamon8888/anno/releases/download/$ResolvedTag"
    $downloadDir = Join-Path -Path $env:TEMP -ChildPath "anno-rag-$ResolvedTag"
    New-Item -ItemType Directory -Force -Path $downloadDir | Out-Null

    $assetPath = Join-Path -Path $downloadDir -ChildPath $asset
    $sumsPath = Join-Path -Path $downloadDir -ChildPath "SHA256SUMS.txt"
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $assetPath
    Invoke-WebRequest -Uri "$base/SHA256SUMS.txt" -OutFile $sumsPath

    $line = Select-String -Path $sumsPath -SimpleMatch $asset | Select-Object -First 1
    if (-not $line) {
        throw "checksum entry not found for $asset"
    }

    $expected = $line.Line.Split()[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $assetPath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "checksum mismatch for $asset"
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Expand-Archive -Path $assetPath -DestinationPath $InstallDir -Force
    $exe = Get-ChildItem -Path $InstallDir -Recurse -File -Filter "anno-rag.exe" | Select-Object -First 1
    if (-not $exe) {
        throw "anno-rag.exe not found after extract"
    }

    return $exe.FullName
}

if ($Source -eq "path") {
    if (-not $Binary) {
        throw "-Binary is required when -Source path"
    }
    $ResolvedBinary = (Resolve-Path -LiteralPath $Binary).Path
} elseif ($Source -eq "local-build") {
    $ResolvedBinary = Resolve-LocalBuildBinary
} else {
    $ResolvedTag = Get-ReleaseTag -RequestedTag $Tag
    $ResolvedBinary = Install-ReleaseBinary -ResolvedTag $ResolvedTag
}

$setupArgs = @("setup-mcp", "--target", $Target, "--binary", $ResolvedBinary, "--models-dir", $ModelsDir)
foreach ($root in $AllowedRoots) {
    $setupArgs += @("--allowed-root", $root)
}
if ($SkipModels) {
    $setupArgs += "--skip-models"
}
if ($DryRun) {
    $setupArgs += "--dry-run"
}
if ($Force) {
    $setupArgs += "--force"
}

& $ResolvedBinary @setupArgs
exit $LASTEXITCODE
