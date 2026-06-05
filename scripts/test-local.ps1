param(
    # Crate to check and test. Required; forces targeting over workspace-wide runs.
    [Parameter(Mandatory=$true)]
    [string]$Package,

    # Extra features to enable (e.g. "gliner2").
    [string[]]$Features = @(),

    # Skip the check step and jump straight to nextest.
    [switch]$TestOnly,

    # Bypass the concurrent-build guard (matches dev-fast.ps1 and loop.ps1 behavior).
    [switch]$Force,

    # Cap cargo compilation jobs. Use 1 for memory-heavy crates such as anno-rag.
    [int]$BuildJobs = 0,

    # Cap nextest runtime concurrency. Use 1 or 2 when local RAM is the bottleneck.
    [string]$TestThreads = "",

    # Nextest profile to use. "local" is fast; "default" is broader.
    [string]$NextestProfile = "local",

    # Run only library unit tests.
    [switch]$LibOnly,

    # Run one or more explicit integration test targets.
    [string[]]$TestTarget = @(),

    # Override Cargo target directory for this run, useful if the configured disk is unstable.
    [string]$TargetDir = "",

    # Use nightly + Cranelift codegen backend for faster test compilation.
    # Requires: rustup toolchain install nightly && rustup component add rustc-codegen-cranelift-preview --toolchain nightly
    [switch]$Cranelift
)

$ErrorActionPreference = "Stop"

# -- Concurrent-build guard -------------------------------------------------
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

# -- Cranelift codegen backend (nightly-only) ----------------------------------
if ($Cranelift) {
    $env:RUSTUP_TOOLCHAIN = "nightly"
    $env:RUSTFLAGS = "-Z codegen-backend=cranelift"
    Write-Host "==> Cranelift enabled (nightly toolchain, -Z codegen-backend=cranelift)" -ForegroundColor Magenta
}

if ($TargetDir -ne "") {
    New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
    $env:CARGO_TARGET_DIR = $TargetDir
    Write-Host "==> CARGO_TARGET_DIR=$TargetDir" -ForegroundColor Cyan
}

# -- Step 1: targeted cargo check ------------------------------------------
if (-not $TestOnly) {
    Write-Host "==> check -p $Package" -ForegroundColor Cyan
    $checkArgs = @("check", "--profile", "dev-fast", "-p", $Package)
    if ($Features.Count -gt 0) {
        $checkArgs += @("--features", ($Features -join ","))
    }
    & cargo @checkArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "cargo check failed; fix errors before running tests." -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

# -- Step 2: nextest with local profile ------------------------------------
Write-Host "==> nextest run -p $Package --profile $NextestProfile" -ForegroundColor Cyan
$testArgs = @("nextest", "run", "-p", $Package)
if ($LibOnly) {
    $testArgs += "--lib"
}
foreach ($target in $TestTarget) {
    $testArgs += @("--test", $target)
}
$testArgs += @("--profile", $NextestProfile, "--cargo-profile", "dev-fast")
if ($BuildJobs -gt 0) {
    $testArgs += @("--build-jobs", "$BuildJobs")
}
if ($TestThreads -ne "") {
    $testArgs += @("--test-threads", "$TestThreads")
}
if ($Features.Count -gt 0) {
    $testArgs += @("--features", ($Features -join ","))
}
& cargo @testArgs
exit $LASTEXITCODE
