param(
    [ValidateSet("check", "build", "install", "all")]
    [string]$Mode = "all",

    [switch]$SkipCheck,

    [switch]$NoInstall,

    [switch]$KillRunning,

    [int]$BuildJobs = 1,

    [string[]]$Features = @(),

    [string]$InstallDir = "$env:LOCALAPPDATA\anno-rag"
)

$ErrorActionPreference = "Stop"

$repoRoot = (& git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repoRoot

if ($KillRunning) {
    Get-Process cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force
} else {
    $running = @(Get-Process cargo,rustc -ErrorAction SilentlyContinue |
        Where-Object { $_.Id -ne $PID })
    if ($running.Count -gt 0) {
        $ids = $running.Id -join ", "
        Write-Host "Concurrent Rust build detected (PIDs: $ids)." -ForegroundColor Red
        Write-Host "Re-run with -KillRunning only if those builds are stale." -ForegroundColor Red
        exit 1
    }
}

if ($BuildJobs -gt 0) {
    $env:CARGO_BUILD_JOBS = "$BuildJobs"
}

$featureArgs = @()
if ($Features.Count -gt 0) {
    $featureArgs = @("--features", ($Features -join ","))
}

function Invoke-CargoStep {
    param([string[]]$CargoArgs)

    Write-Host ("==> cargo " + ($CargoArgs -join " ")) -ForegroundColor Cyan
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

if (($Mode -eq "check" -or $Mode -eq "all" -or $Mode -eq "install") -and -not $SkipCheck) {
    Invoke-CargoStep -CargoArgs (@("check", "-p", "anno-rag-mcp", "--lib") + $featureArgs)
    Invoke-CargoStep -CargoArgs (@("check", "-p", "anno-rag-bin", "--bin", "anno-rag") + $featureArgs)
}

if ($Mode -eq "build" -or $Mode -eq "all" -or $Mode -eq "install") {
    Invoke-CargoStep -CargoArgs (@("build", "-p", "anno-rag-bin", "--bin", "anno-rag") + $featureArgs)
}

if (($Mode -eq "install" -or $Mode -eq "all") -and -not $NoInstall) {
    $metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
    $targetDir = [string]$metadata.target_directory
    $exe = Join-Path $targetDir "debug\anno-rag.exe"
    if (-not (Test-Path -LiteralPath $exe)) {
        Write-Host "Built executable not found: $exe" -ForegroundColor Red
        exit 1
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $dest = Join-Path $InstallDir "anno-rag.exe"
    Copy-Item -LiteralPath $exe -Destination $dest -Force
    Write-Host "Installed MCP binary: $dest" -ForegroundColor Green
}
