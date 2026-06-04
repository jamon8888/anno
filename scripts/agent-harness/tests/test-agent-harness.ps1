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
        [string]$InputPath,
        [string[]]$ExtraArgs = @()
    )
    $scriptPath = Join-Path $HarnessRoot $ScriptName
    if (-not (Test-Path -LiteralPath $scriptPath)) {
        throw "Missing script under test: $scriptPath"
    }
    $inputText = Get-Content -LiteralPath $InputPath -Raw
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
    $psi.WorkingDirectory = $RepoRoot
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

$ran = 0
foreach ($test in $tests) {
    if ($Filter -and ($test.ToString() -notlike "*$Filter*")) {
        continue
    }
    & $test
    $ran += 1
}

Write-Host "agent-harness tests passed: $ran"
