[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Tag,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Target,

    [Parameter(Mandatory = $true)]
    [ValidateSet("cuda")]
    [string]$Flavor
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Test-AssetComponent {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )
    if ($Value -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Invalid $Name`: must match ^[A-Za-z0-9._-]+$"
    }
    if ($Value -notmatch '[A-Za-z0-9]') {
        throw "Invalid $Name`: must contain at least one ASCII alphanumeric character"
    }
}

Test-AssetComponent -Name "Tag" -Value $Tag
Test-AssetComponent -Name "Target" -Value $Target
Test-AssetComponent -Name "Flavor" -Value $Flavor

$ScriptPath = $PSCommandPath
if (-not $ScriptPath) {
    $ScriptPath = $MyInvocation.MyCommand.Path
}

$ReleaseDir = Split-Path -Parent $ScriptPath
$ScriptsDir = Split-Path -Parent $ReleaseDir
$RepoRoot = Split-Path -Parent $ScriptsDir

$PackageName = "hacienda-$Tag-$Target-$Flavor"
$DistDir = Join-Path -Path $RepoRoot -ChildPath "dist"
$StagingDir = Join-Path -Path $DistDir -ChildPath $PackageName
$ExamplesDir = Join-Path -Path $StagingDir -ChildPath "examples"
$ZipPath = Join-Path -Path $DistDir -ChildPath "$PackageName.zip"

$RequiredFiles = @(
    "target/$Target/release/anno-rag.exe",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "env.example",
    "docs/release/accelerated-gpu-builds.md",
    "docs/release/examples/claude_desktop_config.windows.json",
    "docs/release/examples/claude_desktop_config.macos.json"
)

$MissingFiles = @(foreach ($RelativePath in $RequiredFiles) {
    $FullPath = Join-Path -Path $RepoRoot -ChildPath $RelativePath
    if (-not (Test-Path -LiteralPath $FullPath -PathType Leaf)) {
        $RelativePath
    }
})

if ($MissingFiles.Count -gt 0) {
    $MissingList = $MissingFiles -join [Environment]::NewLine
    throw "Cannot create accelerated Windows package. Missing required file(s):$([Environment]::NewLine)$MissingList"
}

New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
if (Test-Path -LiteralPath $StagingDir) {
    Remove-Item -LiteralPath $StagingDir -Recurse -Force
}
if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}

New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null
New-Item -ItemType Directory -Path $ExamplesDir -Force | Out-Null

foreach ($RelativePath in $RequiredFiles) {
    $SourcePath = Join-Path -Path $RepoRoot -ChildPath $RelativePath
    $DestinationDir = $StagingDir
    if ($RelativePath -like "docs/release/examples/*.json") {
        $DestinationDir = $ExamplesDir
    }
    Copy-Item -LiteralPath $SourcePath -Destination $DestinationDir
}

Compress-Archive -Path (Join-Path -Path $StagingDir -ChildPath "*") -DestinationPath $ZipPath -Force
Write-Output $ZipPath
