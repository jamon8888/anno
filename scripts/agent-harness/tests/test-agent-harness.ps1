param(
    [string]$Filter = ""
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$HarnessRoot = Split-Path -Parent $ScriptDir
$RepoRoot = Split-Path -Parent (Split-Path -Parent $HarnessRoot)

function Assert-Equal {
    param(
        [object]$Actual,
        [object]$Expected,
        [string]$Name
    )
    if ($Actual -ne $Expected) {
        throw "ASSERT FAIL: $Name expected '$Expected' but got '$Actual'"
    }
}

function Assert-NotEqual {
    param(
        [object]$Actual,
        [object]$Expected,
        [string]$Name
    )
    if ($Actual -eq $Expected) {
        throw "ASSERT FAIL: $Name expected values to differ, but both were '$Actual'"
    }
}

function Assert-Contains {
    param(
        [string]$Actual,
        [string]$Needle,
        [string]$Name
    )
    if (-not $Actual.Contains($Needle)) {
        throw "ASSERT FAIL: $Name expected output to contain '$Needle'. Output: $Actual"
    }
}

function Invoke-HarnessScript {
    param(
        [string]$ScriptName,
        [string]$InputPath = "",
        [string[]]$ExtraArgs = @(),
        [string]$WorkingDirectory = $RepoRoot
    )
    $scriptPath = Join-Path $HarnessRoot $ScriptName
    if (-not (Test-Path -LiteralPath $scriptPath)) {
        throw "Missing script under test: $scriptPath"
    }
    $inputText = ""
    if (-not [string]::IsNullOrWhiteSpace($InputPath)) {
        $inputText = Get-Content -LiteralPath $InputPath -Raw
    }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "powershell"
    $argParts = New-Object System.Collections.Generic.List[string]
    $argParts.Add("-NoProfile")
    $argParts.Add("-ExecutionPolicy")
    $argParts.Add("Bypass")
    $argParts.Add("-File")
    $argParts.Add(('"{0}"' -f ($scriptPath -replace '"', '\"')))
    foreach ($arg in $ExtraArgs) {
        if ($arg.StartsWith("-")) {
            $argParts.Add($arg)
        } else {
            $argParts.Add(('"{0}"' -f ($arg -replace '"', '\"')))
        }
    }
    $psi.Arguments = $argParts -join " "
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $WorkingDirectory
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Write($inputText)
    $p.StandardInput.Close()
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    [pscustomobject]@{
        ExitCode = $p.ExitCode
        Stdout = $stdout
        Stderr = $stderr
    }
}

$fixtures = Join-Path $ScriptDir "fixtures"
$tests = New-Object System.Collections.Generic.List[scriptblock]

Import-Module (Join-Path $HarnessRoot "lib/AgentHarness.psm1") -Force

$tests.Add({
    $json = Get-Content -LiteralPath (Join-Path $fixtures "pretool-safe.json") -Raw | ConvertFrom-Json
    $command = Get-AgentHarnessCommandText -InputObject $json
    Assert-Equal $command "git status --short" "command extraction"
})

$tests.Add({
    $crate = Get-AgentHarnessCrateFromPath -PathText "crates/anno-rag/src/pipeline.rs"
    Assert-Equal $crate "anno-rag" "crate extraction"
})

$tests.Add({
    $r = Invoke-HarnessScript "block-dangerous-tool.ps1" (Join-Path $fixtures "pretool-dangerous.json")
    Assert-Equal $r.ExitCode 2 "dangerous command is blocked"
    Assert-Contains ($r.Stdout + $r.Stderr) "destructive command" "dangerous command reason"
})

$tests.Add({
    $r = Invoke-HarnessScript "block-dangerous-tool.ps1" (Join-Path $fixtures "pretool-safe.json")
    Assert-Equal $r.ExitCode 0 "safe command is allowed"
})

$tests.Add({
    $r = Invoke-HarnessScript "prompt-secret-scan.ps1" (Join-Path $fixtures "prompt-secret.json")
    Assert-Equal $r.ExitCode 2 "secret prompt is blocked"
    Assert-Contains ($r.Stdout + $r.Stderr) "secret-like" "secret prompt reason"
})

$tests.Add({
    $r = Invoke-HarnessScript "prompt-secret-scan.ps1" (Join-Path $fixtures "prompt-safe-legal.json")
    Assert-Equal $r.ExitCode 0 "ordinary legal prompt is allowed"
})

$tests.Add({
    $r = Invoke-HarnessScript "post-edit-rust-check.ps1" (Join-Path $fixtures "post-edit-rust.json") -ExtraArgs @("-NoRun")
    Assert-Equal $r.ExitCode 0 "post-edit dry mapping passes stdout='$($r.Stdout)' stderr='$($r.Stderr)'"
    Assert-Contains $r.Stdout "anno-rag" "crate detection"
})

$tests.Add({
    $r = Invoke-HarnessScript "crate-map-generate.ps1" -ExtraArgs @("-MetadataPath", (Join-Path $fixtures "cargo-metadata.fixture.json"), "-DryRun")
    Assert-Equal $r.ExitCode 0 "crate map fixture passes stdout='$($r.Stdout)' stderr='$($r.Stderr)'"
    Assert-Contains $r.Stdout "anno-rag-bin depends on anno-rag" "direct workspace dependency"
    Assert-Contains $r.Stdout "anno-rag is depended on by anno-rag-bin" "reverse workspace dependency"
})

$tests.Add({
    $r = Invoke-HarnessScript "cli-feature-parity.ps1" -ExtraArgs @("-DiffNameStatusPath", (Join-Path $fixtures "diff-name-status.fixture.txt"), "-DryRun")
    Assert-Equal $r.ExitCode 0 "cli parity fixture passes stdout='$($r.Stdout)' stderr='$($r.Stderr)'"
    Assert-Contains $r.Stdout "anno-rag change detected" "anno-rag surface detection"
    Assert-Contains $r.Stdout "anno-rag-bin touched" "CLI coverage detection"
})

$tests.Add({
    $r = Invoke-HarnessScript "changelog-generate.ps1" -ExtraArgs @("-CommitsPath", (Join-Path $fixtures "commits.fixture.txt"), "-DryRun")
    Assert-Equal $r.ExitCode 0 "changelog fixture passes stdout='$($r.Stdout)' stderr='$($r.Stderr)'"
    Assert-Contains $r.Stdout "Features" "changelog feature section"
    Assert-Contains $r.Stdout "Bug Fixes" "changelog bug fix section"
})

$tests.Add({
    $r = Invoke-HarnessScript "pr-review-generate.ps1" -ExtraArgs @("-DiffNameStatusPath", (Join-Path $fixtures "diff-name-status.fixture.txt"))
    Assert-Equal $r.ExitCode 0 "pr review fixture passes stdout='$($r.Stdout)' stderr='$($r.Stderr)'"
    Assert-Contains $r.Stdout "Findings" "pr review findings section"
    Assert-Contains $r.Stdout "CLI and MCP Parity" "pr review parity section"
})

$tests.Add({
    $tempRepo = Join-Path ([System.IO.Path]::GetTempPath()) ("agent-harness-test-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRepo | Out-Null
    try {
        git -C $tempRepo init | Out-Null
        git -C $tempRepo config user.email "agent-harness@example.invalid" | Out-Null
        git -C $tempRepo config user.name "Agent Harness Test" | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $tempRepo "crates/anno-rag/src") -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-rag/src/pipeline.rs") -Value "fn main() {}" -Encoding UTF8
        git -C $tempRepo add . | Out-Null
        git -C $tempRepo commit -m "initial" | Out-Null
        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-rag/src/pipeline.rs") -Value "fn main() { println!(`"hi`"); }" -Encoding UTF8

        $files = Get-AgentHarnessChangedRustFiles -Repo $tempRepo
        Assert-Equal ($files -join ",") "crates/anno-rag/src/pipeline.rs" "changed Rust file detection"

        $unstagedFingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $tempRepo -Files $files
        Assert-Equal $unstagedFingerprint.Length 64 "Rust diff fingerprint length"

        git -C $tempRepo add crates/anno-rag/src/pipeline.rs | Out-Null
        $stagedFiles = Get-AgentHarnessChangedRustFiles -Repo $tempRepo
        $stagedFingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $tempRepo -Files $stagedFiles
        Assert-Equal $stagedFingerprint $unstagedFingerprint "staged and unstaged fingerprints match same content"

        New-Item -ItemType Directory -Path (Join-Path $tempRepo "crates/anno-rag-bin/src") -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-rag-bin/src/new_file.rs") -Value "pub fn added() {}" -Encoding UTF8

        $filesWithUntracked = Get-AgentHarnessChangedRustFiles -Repo $tempRepo
        Assert-Equal ($filesWithUntracked -join ",") "crates/anno-rag/src/pipeline.rs,crates/anno-rag-bin/src/new_file.rs" "untracked Rust file detection"

        $crates = Get-AgentHarnessCratesFromPaths -PathText $filesWithUntracked
        Assert-Equal ($crates -join ",") "anno-rag,anno-rag-bin" "changed crate extraction"
    } finally {
        if (Test-Path -LiteralPath $tempRepo) {
            Remove-Item -LiteralPath $tempRepo -Recurse -Force
        }
    }
})

$tests.Add({
    $tempRepo = Join-Path ([System.IO.Path]::GetTempPath()) ("agent-harness-stop-test-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRepo | Out-Null
    try {
        git -C $tempRepo init | Out-Null
        git -C $tempRepo config user.email "agent-harness@example.invalid" | Out-Null
        git -C $tempRepo config user.name "Agent Harness Test" | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $tempRepo "crates/anno-one/src") -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $tempRepo "crates/anno-two/src") -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-one/src/lib.rs") -Value "pub fn one() {}" -Encoding UTF8
        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-two/src/lib.rs") -Value "pub fn two() {}" -Encoding UTF8
        git -C $tempRepo add . | Out-Null
        git -C $tempRepo commit -m "initial" | Out-Null

        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-one/src/lib.rs") -Value "pub fn one() { println!(`"one`"); }" -Encoding UTF8
        Set-Content -LiteralPath (Join-Path $tempRepo "crates/anno-two/src/lib.rs") -Value "pub fn two() { println!(`"two`"); }" -Encoding UTF8

        $changedFiles = @(Get-AgentHarnessChangedRustFiles -Repo $tempRepo)
        $changedCrates = @(Get-AgentHarnessCratesFromPaths -PathText $changedFiles)
        $fingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $tempRepo -Files $changedFiles
        $stateDir = Join-Path $tempRepo ".agent-harness/state"
        New-Item -ItemType Directory -Path $stateDir -Force | Out-Null

        $oneCrateStamp = [ordered]@{
            crate = "anno-one"
            file = "crates/anno-one/src/lib.rs"
            command = "scripts/dev-fast.ps1 -Package anno-one -Mode check"
            checked_crates = @("anno-one")
            changed_rust_files = $changedFiles
            changed_rust_crates = $changedCrates
            rust_diff_fingerprint = $fingerprint
            checked_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        }
        $stampPath = Join-Path $stateDir "last-check.json"
        $oneCrateStamp | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $stampPath -Encoding UTF8

        $blocked = Invoke-HarnessScript "stop-verify.ps1" -WorkingDirectory $tempRepo
        Assert-Equal $blocked.ExitCode 2 "stop gate blocks when one changed crate is unchecked"

        $bothCratesStamp = [ordered]@{
            crate = "anno-one"
            file = "crates/anno-one/src/lib.rs"
            command = "scripts/dev-fast.ps1 -Package anno-one -Mode check"
            checked_crates = $changedCrates
            changed_rust_files = $changedFiles
            changed_rust_crates = $changedCrates
            rust_diff_fingerprint = $fingerprint
            checked_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        }
        $bothCratesStamp | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $stampPath -Encoding UTF8

        $allowed = Invoke-HarnessScript "stop-verify.ps1" -WorkingDirectory $tempRepo
        Assert-Equal $allowed.ExitCode 0 "stop gate allows when all changed crates are checked"
    } finally {
        if (Test-Path -LiteralPath $tempRepo) {
            Remove-Item -LiteralPath $tempRepo -Recurse -Force
        }
    }
})

$tests.Add({
    $tempRepo = Join-Path ([System.IO.Path]::GetTempPath()) ("agent-harness-untracked-test-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRepo | Out-Null
    try {
        git -C $tempRepo init | Out-Null
        git -C $tempRepo config user.email "agent-harness@example.invalid" | Out-Null
        git -C $tempRepo config user.name "Agent Harness Test" | Out-Null
        Set-Content -LiteralPath (Join-Path $tempRepo "README.md") -Value "baseline" -Encoding UTF8
        git -C $tempRepo add . | Out-Null
        git -C $tempRepo commit -m "initial" | Out-Null

        New-Item -ItemType Directory -Path (Join-Path $tempRepo "crates/anno-new/src") -Force | Out-Null
        $untrackedPath = Join-Path $tempRepo "crates/anno-new/src/lib.rs"
        Set-Content -LiteralPath $untrackedPath -Value "pub fn new_value() -> u8 { 1 }" -Encoding UTF8
        $files = @(Get-AgentHarnessChangedRustFiles -Repo $tempRepo)
        $firstFingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $tempRepo -Files $files

        Set-Content -LiteralPath $untrackedPath -Value "pub fn new_value() -> u8 { 2 }" -Encoding UTF8
        $secondFingerprint = Get-AgentHarnessRustDiffFingerprint -Repo $tempRepo -Files $files
        Assert-NotEqual $secondFingerprint $firstFingerprint "untracked Rust file content affects fingerprint"
    } finally {
        if (Test-Path -LiteralPath $tempRepo) {
            Remove-Item -LiteralPath $tempRepo -Recurse -Force
        }
    }
})

$ran = 0
foreach ($test in $tests) {
    if ($Filter -and ($test.ToString() -notlike "*$Filter*")) {
        continue
    }
    & $test
    $ran += 1
}

Write-Host "agent-harness tests passed: $ran"
