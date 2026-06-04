param(
    [switch]$NoBlock
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $repo = Get-AgentHarnessRepoRoot
    $rustChanged = @(Get-AgentHarnessChangedRustFiles -Repo $repo)
    if ($rustChanged.Count -eq 0) {
        Write-Output "agent harness stop gate: no Rust changes detected"
        exit 0
    }

    $stampPath = Join-Path $repo ".agent-harness/state/last-check.json"
    if (Test-Path -LiteralPath $stampPath) {
        $stamp = Get-Content -LiteralPath $stampPath -Raw | ConvertFrom-Json
        $stampFiles = @($stamp.changed_rust_files | ForEach-Object { [string]$_ })
        $currentFingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $repo -Files $rustChanged
        $missingFiles = @($rustChanged | Where-Object { $stampFiles -notcontains $_ })
        if ($stamp.rust_diff_fingerprint -eq $currentFingerprint -and $missingFiles.Count -eq 0) {
            Write-Output "agent harness stop gate: matching targeted check found for $($stamp.crate)"
            exit 0
        }
    }

    $reason = "Rust changes detected without a recent agent harness targeted check. Run scripts/dev-fast.ps1 for the changed crate or explain why verification is impossible."
    if ($NoBlock) {
        Write-Warning $reason
        exit 0
    }
    [Console]::Error.WriteLine($reason)
    Write-AgentHarnessBlockJson -Reason $reason
    exit 2
} catch {
    [Console]::Error.WriteLine("agent harness stop gate error: $($_.Exception.Message)")
    exit 1
}
