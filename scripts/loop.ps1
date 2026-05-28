<#
.SYNOPSIS
    D-loop: check -> nextest -> smoke-run for the anno workspace.

.PARAMETER Package
    Target a specific crate (default: auto-detect from git diff via dev-fast.ps1).

.PARAMETER Smoke
    Run anno-rag-bin --smoke-check after tests pass.

.PARAMETER Since
    Git ref for change detection passed to dev-fast.ps1 (default: HEAD).

.PARAMETER AllAffected
    Run cargo check on all crates that depend on the changed crate.
    cargo nextest still runs only the primary (directly-changed) crate.

.PARAMETER SkipCheck
    Skip the cargo check step and go straight to nextest.
    Used by the VS Code Nextest task (Ctrl+Shift+T).

.EXAMPLE
    scripts\loop.ps1
    scripts\loop.ps1 -Package anno-rag-mcp
    scripts\loop.ps1 -Package anno-rag -AllAffected -Smoke
    scripts\loop.ps1 -SkipCheck -Package anno-rag-mcp

.PARAMETER Force
    Bypass the concurrent-build guard (not recommended).

.NOTES
    Exit codes:
      0 - all steps passed
      1 - cargo check failed
      2 - nextest failed
      3 - smoke-run failed
#>
param(
    [string]$Package  = "",
    [switch]$Smoke,
    [string]$Since    = "HEAD",
    [switch]$AllAffected,
    [switch]$SkipCheck,
    [switch]$Force    # Bypass concurrent-build guard
)

$ErrorActionPreference = "Continue"   # "Stop" breaks on native-cmd stderr (git warnings, cargo warnings)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# ── Concurrent-build guard ─────────────────────────────────────────────────────
if (-not $Force) {
    $running = @(Get-Process cargo, rustc -ErrorAction SilentlyContinue |
                 Where-Object { $_.Id -ne $PID })
    if ($running.Count -gt 0) {
        $ids = $running.Id -join ", "
        Write-Host "[loop] BLOCKED — $($running.Count) cargo/rustc process(es) already running (PIDs: $ids)." -ForegroundColor Red
        Write-Host "       Kill them: Get-Process cargo,rustc | Stop-Process -Force"
        Write-Host "       Override:  scripts\loop.ps1 -Force"
        exit 1
    }
}

# ── Target-dir — enforce SSD ──────────────────────────────────────────────────
if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = "D:\cargo-target"
    Write-Warning "[loop] CARGO_TARGET_DIR not set — defaulting to D:\cargo-target. Verify it is on your SSD."
}

# -----------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------

function Format-Elapsed {
    param([datetime]$Start)
    return "{0:F1}s" -f ((Get-Date) - $Start).TotalSeconds
}

# -----------------------------------------------------------------
# Package detection
# -----------------------------------------------------------------

$primary_package = $Package
$check_packages  = @()

if (-not $primary_package) {
    $raw = & powershell -NoProfile -ExecutionPolicy Bypass `
        -File "$ScriptDir\dev-fast.ps1" -PrintOnly -Since $Since 2>$null
    $pkg_line = $raw | Where-Object { $_ -match '^Packages: ' } | Select-Object -First 1
    if (-not $pkg_line) {
        Write-Host "No changed crates detected. Use -Package <crate> to target manually."
        exit 0
    }
    $names = ($pkg_line -replace '^Packages: ', '') -split ', ' |
        ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }
    $primary_package = $names[0]
}

$check_packages = @($primary_package)

if ($AllAffected) {
    $raw_all = & powershell -NoProfile -ExecutionPolicy Bypass `
        -File "$ScriptDir\dev-fast.ps1" -AllAffected -PrintOnly -Since $Since 2>$null
    $pkg_line_all = $raw_all | Where-Object { $_ -match '^Packages: ' } | Select-Object -First 1
    if ($pkg_line_all) {
        $check_packages = ($pkg_line_all -replace '^Packages: ', '') -split ', ' |
            ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }
    }
}

$total_start = Get-Date

# -----------------------------------------------------------------
# Step 1 - cargo check
# -----------------------------------------------------------------

if (-not $SkipCheck) {
    $check_label = $check_packages -join ', '
    Write-Host "[1/3] check $check_label ..." -NoNewline
    $step_start = Get-Date

    $check_ok  = $true
    $check_out = @()
    foreach ($pkg in $check_packages) {
        $out = cargo check --profile dev-fast -p $pkg 2>&1
        if ($LASTEXITCODE -ne 0) {
            $check_ok  = $false
            $check_out += $out
            break
        }
    }

    $elapsed = Format-Elapsed $step_start
    if (-not $check_ok) {
        Write-Host "  $elapsed  X"
        $check_out | ForEach-Object { Write-Host $_ }
        exit 1
    }
    Write-Host "  $elapsed  OK"
} else {
    Write-Host "[1/3] check skipped (-SkipCheck)"
}

# -----------------------------------------------------------------
# Step 2 - cargo nextest
# -----------------------------------------------------------------

Write-Host "[2/3] nextest $primary_package ..." -NoNewline
$step_start  = Get-Date

$nextest_out  = cargo nextest run --profile dev-fast -p $primary_package 2>&1
$nextest_exit = $LASTEXITCODE
$elapsed      = Format-Elapsed $step_start

if ($nextest_exit -ne 0) {
    Write-Host "  $elapsed  X"
    $nextest_out | ForEach-Object { Write-Host $_ }
    exit 2
}

# Parse test count from summary line: "N tests run: N passed, 0 skipped"
$summary = $nextest_out | Where-Object { $_ -match '(\d+) tests? run:' } | Select-Object -Last 1
$count_label = ""
if ($summary -match '(\d+) tests? run:') {
    $count_label = "  ($($Matches[1]) tests)"
}

Write-Host "  $elapsed  OK$count_label"

# -----------------------------------------------------------------
# Step 3 - smoke run (optional)
# -----------------------------------------------------------------

if ($Smoke) {
    Write-Host "[3/3] smoke ..." -NoNewline
    $step_start = Get-Date

    $smoke_out = & anno-rag-bin --smoke-check 2>&1
    $elapsed   = Format-Elapsed $step_start

    if ($LASTEXITCODE -ne 0) {
        Write-Host "  $elapsed  X"
        $smoke_out | ForEach-Object { Write-Host $_ }
        exit 3
    }

    Write-Host "  $elapsed  OK"
} else {
    Write-Host "[3/3] smoke skipped (no -Smoke flag)"
}

# -----------------------------------------------------------------
# Summary
# -----------------------------------------------------------------

$total_elapsed = Format-Elapsed $total_start
Write-Host "Total: $total_elapsed"
exit 0
