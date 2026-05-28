param(
    # Crate to check and test. Required — forces targeting over workspace-wide runs.
    [Parameter(Mandatory=$true)]
    [string]$Package,

    # Extra features to enable (e.g. "gliner2").
    [string[]]$Features = @(),

    # Skip the check step and jump straight to nextest.
    [switch]$TestOnly,

    # Bypass the concurrent-build guard (matches dev-fast.ps1 and loop.ps1 behavior).
    [switch]$Force
)

$ErrorActionPreference = "Stop"

# ── Concurrent-build guard ─────────────────────────────────────────────────
if (-not $Force) {
    $running = @(Get-Process cargo, rustc -ErrorAction SilentlyContinue |
                 Where-Object { $_.Id -ne $PID })
    if ($running.Count -gt 0) {
        $ids = $running.Id -join ", "
        Write-Host "Concurrent build detected (PIDs: $ids). Kill them first: Get-Process cargo,rustc | Stop-Process -Force" -ForegroundColor Red
        exit 1
    }
}

$repoRoot = (& git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repoRoot

# ── Step 1: targeted cargo check ──────────────────────────────────────────
if (-not $TestOnly) {
    Write-Host "==> check -p $Package" -ForegroundColor Cyan
    $checkArgs = @("check", "--profile", "dev-fast", "-p", $Package)
    if ($Features.Count -gt 0) {
        $checkArgs += @("--features", ($Features -join ","))
    }
    & cargo @checkArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "cargo check failed — fix errors before running tests." -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

# ── Step 2: nextest with local profile ────────────────────────────────────
Write-Host "==> nextest run -p $Package --profile local" -ForegroundColor Cyan
$testArgs = @("nextest", "run", "-p", $Package, "--profile", "local", "--cargo-profile", "dev-fast")
if ($Features.Count -gt 0) {
    $testArgs += @("--features", ($Features -join ","))
}
& cargo @testArgs
exit $LASTEXITCODE
