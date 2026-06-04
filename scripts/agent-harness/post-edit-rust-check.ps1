param(
    [switch]$NoRun
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $repo = Get-AgentHarnessRepoRoot
    $inputObject = Read-AgentHarnessJsonFromStdin
    $pathText = Get-AgentHarnessFilePath -InputObject $inputObject
    $crate = Get-AgentHarnessCrateFromPath -PathText $pathText

    if (-not $pathText.EndsWith(".rs")) {
        Write-Output "agent harness: no Rust file detected"
        exit 0
    }
    if (-not $crate) {
        Write-Output "agent harness: Rust path is outside crates/: $pathText"
        exit 0
    }

    Write-Output "agent harness: detected crate $crate from $pathText"
    if ($NoRun) {
        exit 0
    }

    $fullPath = Join-Path $repo $pathText
    if (Test-Path -LiteralPath $fullPath) {
        rustfmt --edition 2021 $fullPath
        if ($LASTEXITCODE -ne 0) {
            throw "rustfmt failed with exit code $LASTEXITCODE"
        }
    }

    $devFast = Join-Path $repo "scripts/dev-fast.ps1"
    powershell -NoProfile -ExecutionPolicy Bypass -File $devFast -Package $crate -Mode check
    if ($LASTEXITCODE -ne 0) {
        throw "scripts/dev-fast.ps1 failed with exit code $LASTEXITCODE"
    }

    $changedRustFiles = @(Get-AgentHarnessChangedRustFiles -Repo $repo)
    $fingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $repo -Files $changedRustFiles

    $stateDir = Join-Path $repo ".agent-harness/state"
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    $stamp = [ordered]@{
        crate = $crate
        file = $pathText
        command = "scripts/dev-fast.ps1 -Package $crate -Mode check"
        changed_rust_files = $changedRustFiles
        rust_diff_fingerprint = $fingerprint
        checked_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    }
    $stamp | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $stateDir "last-check.json") -Encoding UTF8
    exit 0
} catch {
    [Console]::Error.WriteLine("agent harness post-edit hook error: $($_.Exception.Message)")
    exit 1
}
