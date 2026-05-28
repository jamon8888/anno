param(
    # Crate to check and test. Required — forces targeting over workspace-wide runs.
    [Parameter(Mandatory=$true)]
    [string]$Package,

    # Extra features to enable (e.g. "gliner2").
    [string[]]$Features = @(),

    # Skip the check step and jump straight to nextest.
    [switch]$TestOnly
)

$ErrorActionPreference = "Stop"

# ── Concurrent-build guard ─────────────────────────────────────────────────
$running = @(Get-Process cargo, rustc -ErrorAction SilentlyContinue |
             Where-Object { $_.Id -ne $PID })
if ($running.Count -gt 0) {
    $ids = $running.Id -join ", "
    Write-Error "Concurrent build detected (PIDs: $ids). Kill them first: Get-Process cargo,rustc | Stop-Process -Force"
    exit 1
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
        Write-Error "cargo check failed — fix errors before running tests."
        exit $LASTEXITCODE
    }
}

# ── Step 2: nextest with local profile ────────────────────────────────────
Write-Host "==> nextest run -p $Package --profile local" -ForegroundColor Cyan
$testArgs = @("nextest", "run", "-p", $Package, "--profile", "local")
if ($Features.Count -gt 0) {
    $testArgs += @("--features", ($Features -join ","))
}
& cargo @testArgs
exit $LASTEXITCODE
