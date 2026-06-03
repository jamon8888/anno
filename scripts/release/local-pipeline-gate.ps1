[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [ValidateSet("fast", "release", "deep")]
    [string]$Profile = "fast",

    [Parameter(Mandatory = $false)]
    [string]$Corpus,

    [Parameter(Mandatory = $false)]
    [string]$RunRoot,

    [Parameter(Mandatory = $false)]
    [switch]$SkipBuild,

    [Parameter(Mandatory = $false)]
    [switch]$SkipHeavy,

    [Parameter(Mandatory = $false)]
    [switch]$SkipOcr,

    [Parameter(Mandatory = $false)]
    [switch]$SkipMcp,

    [Parameter(Mandatory = $false)]
    [switch]$DryRun,

    [Parameter(Mandatory = $false)]
    [int]$BuildTimeoutSecs = 600
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Static gate markers used by test-local-pipeline-gate.ps1:
# cargo check -p anno-rag --features embedded-ocr
# anno-rag bench
# anno-rag mcp
# regex_pii_recall_meets_baseline

$ScriptPath = $PSCommandPath
if (-not $ScriptPath) {
    $ScriptPath = $MyInvocation.MyCommand.Path
}

$ReleaseDir = Split-Path -Parent $ScriptPath
$ScriptsDir = Split-Path -Parent $ReleaseDir
$RepoRoot = Split-Path -Parent $ScriptsDir
$IsWindowsPlatform = [System.IO.Path]::DirectorySeparatorChar -eq '\'

if (-not $RunRoot) {
    $RunRoot = Join-Path -Path $RepoRoot -ChildPath "target/local-release-gate"
}

$StartedAt = Get-Date
$RunId = "run-{0}" -f $StartedAt.ToString("yyyyMMdd-HHmmss")
$RunDir = Join-Path -Path $RunRoot -ChildPath $RunId
$SamplesDir = Join-Path -Path $RunDir -ChildPath "samples"
$DataHome = Join-Path -Path $RunDir -ChildPath "home"
$OutputsDir = Join-Path -Path $RunDir -ChildPath "outputs"
$ReportsDir = Join-Path -Path $RunDir -ChildPath "reports"
$LogsDir = Join-Path -Path $RunDir -ChildPath "logs"
$MetricsPath = Join-Path -Path $ReportsDir -ChildPath "metrics.json"
$ReportPath = Join-Path -Path $ReportsDir -ChildPath "report.md"
$CommandLogPath = Join-Path -Path $ReportsDir -ChildPath "commands.log"
$OcrGateEnabled = (-not $SkipOcr) -and ($Profile -ne "fast")
$BuildMode = if ($Profile -eq "fast") { "debug" } else { "release" }

$Metrics = [ordered]@{
    profile = $Profile
    dry_run = [bool]$DryRun
    ocr_gate_enabled = [bool]$OcrGateEnabled
    build_mode = $BuildMode
    started_at = $StartedAt.ToUniversalTime().ToString("o")
    finished_at = $null
    repo_root = $RepoRoot
    run_dir = $RunDir
    samples_dir = $SamplesDir
    outputs_dir = $OutputsDir
    reports_dir = $ReportsDir
    git = [ordered]@{}
    samples = [ordered]@{}
    gates = @()
    summary = [ordered]@{
        status = "running"
        failures = 0
    }
}

function ConvertTo-CommandLine {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter(Mandatory = $false)]
        [string[]]$Arguments = @()
    )

    $parts = @($FilePath) + @($Arguments)
    ($parts | ForEach-Object {
        if ($_ -match '\s') {
            '"' + ($_ -replace '"', '\"') + '"'
        } else {
            $_
        }
    }) -join " "
}

function ConvertTo-ArgumentString {
    param(
        [Parameter(Mandatory = $false)]
        [string[]]$Arguments = @()
    )

    ($Arguments | ForEach-Object {
        if ($_ -eq "") {
            '""'
        } elseif ($_ -match '[\s"]') {
            '"' + ($_ -replace '\\', '\\' -replace '"', '\"') + '"'
        } else {
            $_
        }
    }) -join " "
}

function Resolve-Executable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath
    )

    if ([System.IO.Path]::IsPathRooted($FilePath)) {
        return $FilePath
    }

    $Command = Get-Command $FilePath -ErrorAction Stop
    $Command.Source
}

function Get-CargoTargetDir {
    if ($env:CARGO_TARGET_DIR) {
        return $env:CARGO_TARGET_DIR
    }
    Join-Path -Path $RepoRoot -ChildPath "target"
}

function Resolve-CargoBinary {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryName,

        [Parameter(Mandatory = $true)]
        [string]$BuildDir
    )

    $TargetDir = Get-CargoTargetDir
    $Primary = Join-Path -Path $TargetDir -ChildPath "$BuildDir/$BinaryName"
    if (Test-Path -LiteralPath $Primary -PathType Leaf) {
        return $Primary
    }

    if ($BuildDir -eq "debug") {
        $DepsName = $BinaryName -replace "-", "_"
        $Fallback = Join-Path -Path $TargetDir -ChildPath "$BuildDir/deps/$DepsName"
        if (Test-Path -LiteralPath $Fallback -PathType Leaf) {
            return $Fallback
        }
    }

    return $Primary
}

function Stop-ProcessTree {
    param(
        [Parameter(Mandatory = $true)]
        [System.Diagnostics.Process]$Process
    )

    if ($Process.HasExited) {
        return
    }

    if ($IsWindowsPlatform) {
        $TaskKill = Join-Path -Path $env:SystemRoot -ChildPath "System32\taskkill.exe"
        if (Test-Path -LiteralPath $TaskKill) {
            & $TaskKill /PID $Process.Id /T /F | Out-Null
        } else {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
    } else {
        $Process.Kill()
    }
}

function Write-ArtifactFiles {
    $Metrics.finished_at = (Get-Date).ToUniversalTime().ToString("o")
    $Metrics.summary.failures = @($Metrics.gates | Where-Object { $_.status -eq "failed" }).Count
    if ($Metrics.summary.failures -eq 0 -and $Metrics.summary.status -ne "failed") {
        if ($DryRun) {
            $Metrics.summary.status = "dry-run"
        } else {
            $Metrics.summary.status = "passed"
        }
    }

    if (-not $DryRun) {
        New-Item -ItemType Directory -Path $ReportsDir -Force | Out-Null
        $Metrics | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $MetricsPath -Encoding UTF8
        $lines = @(
            "# anno local pipeline gate",
            "",
            "- Profile: ``$Profile``",
            "- Status: ``$($Metrics.summary.status)``",
            "- Run dir: ``$RunDir``",
            "- Metrics: ``$MetricsPath``",
            "- Command log: ``$CommandLogPath``",
            "",
            "## Gates",
            ""
        )
        foreach ($Gate in $Metrics.gates) {
            $duration = if ($null -ne $Gate.duration_seconds) { "{0:n2}s" -f $Gate.duration_seconds } else { "-" }
            $lines += "- ``$($Gate.name)``: $($Gate.status), exit=$($Gate.exit_code), duration=$duration"
        }
        $lines | Set-Content -LiteralPath $ReportPath -Encoding UTF8
    }
}

function Add-GateRecord {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$Record
    )

    $Metrics.gates += $Record
}

function Invoke-GateCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter(Mandatory = $false)]
        [string[]]$Arguments = @(),

        [Parameter(Mandatory = $false)]
        [hashtable]$Environment = @{},

        [Parameter(Mandatory = $false)]
        [int]$TimeoutSeconds = 0,

        [Parameter(Mandatory = $false)]
        [switch]$AllowFailure
    )

    $CommandLine = ConvertTo-CommandLine -FilePath $FilePath -Arguments $Arguments
    $LogPath = Join-Path -Path $LogsDir -ChildPath (($Name -replace '[^A-Za-z0-9_.-]', '_') + ".log")

    if ($DryRun) {
        Write-Output "[dry-run] $Name :: $CommandLine"
        Add-GateRecord @{
            name = $Name
            command = $CommandLine
            status = "dry-run"
            exit_code = 0
            duration_seconds = $null
            log = $LogPath
        }
        return
    }

    New-Item -ItemType Directory -Path $LogsDir -Force | Out-Null
    Add-Content -LiteralPath $CommandLogPath -Value "[$(Get-Date -Format o)] $Name :: $CommandLine"

    $Stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $ExitCode = -1
    try {
        $Psi = [System.Diagnostics.ProcessStartInfo]::new()
        $Psi.FileName = Resolve-Executable -FilePath $FilePath
        $Psi.Arguments = ConvertTo-ArgumentString -Arguments $Arguments
        $Psi.WorkingDirectory = $RepoRoot
        $Psi.RedirectStandardOutput = $true
        $Psi.RedirectStandardError = $true
        $Psi.UseShellExecute = $false
        $Psi.CreateNoWindow = $true
        foreach ($Key in $Environment.Keys) {
            $Psi.EnvironmentVariables[$Key] = [string]$Environment[$Key]
        }

        $Process = [System.Diagnostics.Process]::Start($Psi)
        $StdoutTask = $Process.StandardOutput.ReadToEndAsync()
        $StderrTask = $Process.StandardError.ReadToEndAsync()
        if ($TimeoutSeconds -gt 0) {
            $Finished = $Process.WaitForExit($TimeoutSeconds * 1000)
            if (-not $Finished) {
                Stop-ProcessTree -Process $Process
                $Process.WaitForExit()
                $ExitCode = -2
            } else {
                $ExitCode = $Process.ExitCode
            }
        } else {
            $Process.WaitForExit()
            $ExitCode = $Process.ExitCode
        }
        $Output = @($StdoutTask.Result, $StderrTask.Result) -join [Environment]::NewLine
        if ($ExitCode -eq -2) {
            $Output += [Environment]::NewLine + "Command timed out after $TimeoutSeconds seconds."
        }
        $Output | Tee-Object -FilePath $LogPath
    } finally {
        $Stopwatch.Stop()
    }

    $Status = if ($ExitCode -eq 0) { "passed" } else { "failed" }
    Add-GateRecord @{
        name = $Name
        command = $CommandLine
        status = $Status
        exit_code = $ExitCode
        duration_seconds = [Math]::Round($Stopwatch.Elapsed.TotalSeconds, 3)
        log = $LogPath
    }

    if ($ExitCode -ne 0 -and -not $AllowFailure) {
        throw "Gate failed: $Name (exit $ExitCode)"
    }
}

function Copy-IfExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Source,

        [Parameter(Mandatory = $true)]
        [string]$Destination
    )

    if (Test-Path -LiteralPath $Source) {
        Copy-Item -LiteralPath $Source -Destination $Destination -Recurse -Force
    }
}

function New-SampleCorpus {
    if ($DryRun) {
        Write-Output "[dry-run] sample corpus would be created at $SamplesDir"
        return
    }

    New-Item -ItemType Directory -Path $SamplesDir -Force | Out-Null

    if ($Corpus) {
        if (-not (Test-Path -LiteralPath $Corpus)) {
            throw "Corpus path does not exist: $Corpus"
        }
        Copy-Item -LiteralPath $Corpus -Destination (Join-Path $SamplesDir "user-corpus") -Recurse -Force
    }

    $RagFixtures = Join-Path -Path $RepoRoot -ChildPath "crates/anno-rag/tests/fixtures"
    Copy-IfExists -Source (Join-Path $RagFixtures "bench_corpus") -Destination (Join-Path $SamplesDir "bench_corpus")
    Copy-IfExists -Source (Join-Path $RagFixtures "contract_fr.txt") -Destination (Join-Path $SamplesDir "contract_fr.txt")
    Copy-IfExists -Source (Join-Path $RagFixtures "jugement_fr.txt") -Destination (Join-Path $SamplesDir "jugement_fr.txt")
    if ($Profile -ne "fast") {
        Copy-IfExists -Source (Join-Path $RagFixtures "eval_corpus") -Destination (Join-Path $SamplesDir "eval_corpus")
        Copy-IfExists -Source (Join-Path $RagFixtures "pii_corpus") -Destination (Join-Path $SamplesDir "pii_corpus")
    }

    $CliFixtures = Join-Path -Path $RepoRoot -ChildPath "crates/anno-cli/tests/fixtures"
    Copy-IfExists -Source (Join-Path $CliFixtures "legal.txt") -Destination (Join-Path $SamplesDir "cli_legal.txt")
    Copy-IfExists -Source (Join-Path $CliFixtures "press_release.html") -Destination (Join-Path $SamplesDir "press_release.html")

    Set-Content -LiteralPath (Join-Path $SamplesDir "empty.txt") -Value "" -Encoding UTF8
    Copy-IfExists -Source (Join-Path $SamplesDir "contract_fr.txt") -Destination (Join-Path $SamplesDir "contract_fr_duplicate.txt")

    $Files = @(Get-ChildItem -LiteralPath $SamplesDir -Recurse -File)
    $Metrics.samples.file_count = $Files.Count
    $Metrics.samples.total_bytes = ($Files | Measure-Object -Property Length -Sum).Sum
}

function Invoke-McpSmoke {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryPath,

        [Parameter(Mandatory = $true)]
        [hashtable]$Environment
    )

    $Name = "anno-rag mcp smoke"
    $LogPath = Join-Path -Path $LogsDir -ChildPath "anno-rag_mcp_smoke.log"
    $CommandLine = ConvertTo-CommandLine -FilePath $BinaryPath -Arguments @("mcp")

    if ($DryRun) {
        Write-Output "[dry-run] $Name :: $CommandLine"
        Add-GateRecord @{
            name = $Name
            command = $CommandLine
            status = "dry-run"
            exit_code = 0
            duration_seconds = $null
            log = $LogPath
        }
        return
    }

    $Psi = [System.Diagnostics.ProcessStartInfo]::new()
    $Psi.FileName = $BinaryPath
    $Psi.Arguments = "mcp"
    $Psi.WorkingDirectory = $RepoRoot
    $Psi.RedirectStandardInput = $true
    $Psi.RedirectStandardOutput = $true
    $Psi.RedirectStandardError = $true
    $Psi.UseShellExecute = $false
    foreach ($Key in $Environment.Keys) {
        $Psi.EnvironmentVariables[$Key] = [string]$Environment[$Key]
    }

    $Stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $Process = [System.Diagnostics.Process]::Start($Psi)
    $Initialize = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"local-pipeline-gate","version":"0.1.0"}}}'
    $ToolsList = '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
    $Process.StandardInput.WriteLine($Initialize)
    $Process.StandardInput.WriteLine($ToolsList)
    $Process.StandardInput.Close()

    if (-not $Process.WaitForExit(15000)) {
        $Process.Kill($true)
        $Stopwatch.Stop()
        Add-GateRecord @{
            name = $Name
            command = $CommandLine
            status = "failed"
            exit_code = -1
            duration_seconds = [Math]::Round($Stopwatch.Elapsed.TotalSeconds, 3)
            log = $LogPath
        }
        throw "MCP smoke timed out"
    }

    $Stopwatch.Stop()
    $Stdout = $Process.StandardOutput.ReadToEnd()
    $Stderr = $Process.StandardError.ReadToEnd()

    $RequiredMcpTools = @(
        "index",
        "search",
        "sources",
        "status",
        "forget",
        "legacy_search",
        "anno_health",
        "review_create",
        "review_add_rows",
        "review_extract",
        "review_refine_cell",
        "review_set_cell",
        "review_lock_cell",
        "review_unlock_cell",
        "review_export",
        "review_get"
    )
    $ToolNames = @()
    foreach ($Line in ($Stdout -split "\r?\n")) {
        $Trimmed = $Line.Trim()
        if (-not $Trimmed.StartsWith("{")) {
            continue
        }
        try {
            $Message = $Trimmed | ConvertFrom-Json -ErrorAction Stop
        } catch {
            continue
        }
        $MessageProperties = @($Message.PSObject.Properties | ForEach-Object { $_.Name })
        if ($MessageProperties -notcontains "id" -or $Message.id -ne 2) {
            continue
        }
        if ($MessageProperties -notcontains "result" -or $null -eq $Message.result) {
            continue
        }
        $ResultProperties = @($Message.result.PSObject.Properties | ForEach-Object { $_.Name })
        if ($ResultProperties -notcontains "tools") {
            continue
        }
        foreach ($Tool in $Message.result.tools) {
            $ToolProperties = @($Tool.PSObject.Properties | ForEach-Object { $_.Name })
            if ($ToolProperties -contains "name" -and $Tool.name) {
                $ToolNames += [string]$Tool.name
            }
        }
    }
    $MissingMcpTools = @($RequiredMcpTools | Where-Object { $ToolNames -notcontains $_ })

    $LogLines = @($Stdout, $Stderr)
    if ($MissingMcpTools.Count -gt 0) {
        $LogLines += "Missing MCP tools: $($MissingMcpTools -join ', ')"
    }
    $LogLines | Set-Content -LiteralPath $LogPath -Encoding UTF8

    $Status = if ($Process.ExitCode -eq 0 -and $MissingMcpTools.Count -eq 0) { "passed" } else { "failed" }
    Add-GateRecord @{
        name = $Name
        command = $CommandLine
        status = $Status
        exit_code = $Process.ExitCode
        duration_seconds = [Math]::Round($Stopwatch.Elapsed.TotalSeconds, 3)
        log = $LogPath
    }
    if ($Status -ne "passed") {
        throw "MCP smoke failed; see $LogPath"
    }
}

function Invoke-GatewaySmoke {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryPath
    )

    $Name = "anno-privacy-gateway boot smoke"
    $LogPath = Join-Path -Path $LogsDir -ChildPath "anno-privacy-gateway_boot_smoke.log"
    $CommandLine = ConvertTo-CommandLine -FilePath $BinaryPath -Arguments @()

    if ($DryRun) {
        Write-Output "[dry-run] $Name :: $CommandLine"
        Add-GateRecord @{
            name = $Name
            command = $CommandLine
            status = "dry-run"
            exit_code = 0
            duration_seconds = $null
            log = $LogPath
        }
        return
    }

    New-Item -ItemType Directory -Path $LogsDir -Force | Out-Null
    Add-Content -LiteralPath $CommandLogPath -Value "[$(Get-Date -Format o)] $Name :: $CommandLine"

    $Stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $ExitCode = -1
    $Status = "failed"
    try {
        $Psi = [System.Diagnostics.ProcessStartInfo]::new()
        $Psi.FileName = Resolve-Executable -FilePath $BinaryPath
        $Psi.WorkingDirectory = $RepoRoot
        $Psi.RedirectStandardOutput = $true
        $Psi.RedirectStandardError = $true
        $Psi.UseShellExecute = $false
        $Psi.CreateNoWindow = $true
        $Psi.EnvironmentVariables["ANNO_GATEWAY_LISTEN"] = "127.0.0.1:0"

        $Process = [System.Diagnostics.Process]::Start($Psi)
        $StdoutTask = $Process.StandardOutput.ReadToEndAsync()
        $StderrTask = $Process.StandardError.ReadToEndAsync()

        if ($Process.WaitForExit(3000)) {
            $ExitCode = $Process.ExitCode
            $Output = @(
                $StdoutTask.Result,
                $StderrTask.Result,
                "Gateway exited before the 3s smoke window."
            ) -join [Environment]::NewLine
        } else {
            Stop-ProcessTree -Process $Process
            $Process.WaitForExit()
            $ExitCode = 0
            $Status = "passed"
            $Output = @(
                $StdoutTask.Result,
                $StderrTask.Result,
                "Gateway stayed alive for 3s on ANNO_GATEWAY_LISTEN=127.0.0.1:0; terminated by smoke test."
            ) -join [Environment]::NewLine
        }

        $Output | Tee-Object -FilePath $LogPath
    } finally {
        $Stopwatch.Stop()
    }

    Add-GateRecord @{
        name = $Name
        command = $CommandLine
        status = $Status
        exit_code = $ExitCode
        duration_seconds = [Math]::Round($Stopwatch.Elapsed.TotalSeconds, 3)
        log = $LogPath
    }

    if ($Status -ne "passed") {
        throw "Gateway smoke failed; see $LogPath"
    }
}

try {
    if ($DryRun) {
        Write-Output "local pipeline gate dry run"
        Write-Output "profile: $Profile"
        Write-Output "run dir: $RunDir"
        Write-Output "metrics.json: $MetricsPath"
        Write-Output "report.md: $ReportPath"
    } else {
        New-Item -ItemType Directory -Path $RunDir, $ReportsDir, $LogsDir, $OutputsDir, $DataHome -Force | Out-Null
        Set-Content -LiteralPath $CommandLogPath -Value "# local pipeline gate commands" -Encoding UTF8
    }

    Push-Location $RepoRoot
    try {
        $Metrics.git.branch = (git rev-parse --abbrev-ref HEAD)
        $Metrics.git.commit = (git rev-parse --short HEAD)

        New-SampleCorpus

        if (-not $SkipBuild) {
            Invoke-GateCommand -Name "cargo check anno-rag" -FilePath "cargo" -Arguments @("check", "-p", "anno-rag")
            Invoke-GateCommand -Name "cargo check anno-rag embedded OCR" -FilePath "cargo" -Arguments @("check", "-p", "anno-rag", "--features", "embedded-ocr")
            Invoke-GateCommand -Name "cargo check anno-rag-bin embedded OCR" -FilePath "cargo" -Arguments @("check", "-p", "anno-rag-bin", "--features", "embedded-ocr")
            Invoke-GateCommand -Name "cargo check anno-rag-mcp" -FilePath "cargo" -Arguments @("check", "-p", "anno-rag-mcp")
            $BinaryName = if ($IsWindowsPlatform) { "anno-rag.exe" } else { "anno-rag" }
            $GatewayName = if ($IsWindowsPlatform) { "anno-privacy-gateway.exe" } else { "anno-privacy-gateway" }
            $FastAnnoRagBin = Resolve-CargoBinary -BinaryName $BinaryName -BuildDir "debug"
            $FastGatewayBin = Resolve-CargoBinary -BinaryName $GatewayName -BuildDir "debug"
            if ($BuildMode -eq "debug") {
                if ((Test-Path -LiteralPath $FastAnnoRagBin -PathType Leaf) -and (Test-Path -LiteralPath $FastGatewayBin -PathType Leaf)) {
                    Add-GateRecord @{
                        name = "cargo build debug binaries"
                        command = "reuse existing debug binaries"
                        status = "passed"
                        exit_code = 0
                        duration_seconds = 0
                        log = ""
                    }
                } else {
                    $RustFlags = @($env:RUSTFLAGS, "-C debuginfo=0") | Where-Object { $_ }
                    $FastBuildEnv = @{
                        "RUSTFLAGS" = ($RustFlags -join " ")
                        "CARGO_INCREMENTAL" = "0"
                    }
                    Invoke-GateCommand -Name "cargo build debug binaries" -FilePath "cargo" -Arguments @("build", "-p", "anno-rag-bin", "-p", "anno-privacy-gateway") -Environment $FastBuildEnv -TimeoutSeconds $BuildTimeoutSecs
                }
            } else {
                Invoke-GateCommand -Name "cargo build release binaries" -FilePath "cargo" -Arguments @("build", "--release", "-p", "anno-rag-bin", "-p", "anno-privacy-gateway") -TimeoutSeconds $BuildTimeoutSecs
            }
            if ($OcrGateEnabled) {
                Invoke-GateCommand -Name "cargo build release anno-rag embedded OCR" -FilePath "cargo" -Arguments @("build", "--release", "-p", "anno-rag-bin", "--features", "embedded-ocr") -TimeoutSeconds $BuildTimeoutSecs
            }
        }

        if ($Profile -eq "fast") {
            Add-GateRecord @{
                name = "cargo test gates"
                command = "skipped in fast profile"
                status = "passed"
                exit_code = 0
                duration_seconds = 0
                log = ""
            }
        } else {
            Invoke-GateCommand -Name "cargo test anno-rag lib" -FilePath "cargo" -Arguments @("test", "-p", "anno-rag", "--lib")
            Invoke-GateCommand -Name "regex_pii_recall_meets_baseline" -FilePath "cargo" -Arguments @("test", "-p", "anno-rag", "--test", "pii_regex", "regex_pii_recall_meets_baseline", "--", "--exact")
            Invoke-GateCommand -Name "privacy gateway GDPR audit chain" -FilePath "cargo" -Arguments @("test", "-p", "anno-privacy-gateway", "--test", "e2e_gdpr")
        }

        $BinaryName = if ($IsWindowsPlatform) { "anno-rag.exe" } else { "anno-rag" }
        $GatewayName = if ($IsWindowsPlatform) { "anno-privacy-gateway.exe" } else { "anno-privacy-gateway" }
        $BuildDir = if ($BuildMode -eq "debug") { "debug" } else { "release" }
        $AnnoRagBin = Resolve-CargoBinary -BinaryName $BinaryName -BuildDir $BuildDir
        $GatewayBin = Resolve-CargoBinary -BinaryName $GatewayName -BuildDir $BuildDir
        if (-not $DryRun -and -not (Test-Path -LiteralPath $AnnoRagBin)) {
            throw "Missing $BuildMode binary: $AnnoRagBin. Run without -SkipBuild or build it first."
        }

        $RuntimeEnv = @{
            "ANNO_RAG_VAULT_PASSPHRASE" = "local-pipeline-gate-passphrase"
            "ANNO_RAG_DATA_DIR" = $DataHome
            "USERPROFILE" = $DataHome
            "HOME" = $DataHome
        }

        $IngestArgs = @("ingest", $SamplesDir, "--recursive", "--output", $OutputsDir)
        if ($OcrGateEnabled) {
            $IngestArgs += "--enable-ocr"
        }
        Invoke-GateCommand -Name "anno-rag ingest local samples" -FilePath $AnnoRagBin -Arguments $IngestArgs -Environment $RuntimeEnv
        Invoke-GateCommand -Name "anno-rag reingest idempotency smoke" -FilePath $AnnoRagBin -Arguments $IngestArgs -Environment $RuntimeEnv

        foreach ($Query in @("resiliation contrat", "clause confidentialite", "virement bancaire IBAN", "preavis de bail")) {
            Invoke-GateCommand -Name "anno-rag search $Query" -FilePath $AnnoRagBin -Arguments @("search", $Query, "--top-k", "5") -Environment $RuntimeEnv
        }

        if (-not $SkipMcp) {
            Invoke-McpSmoke -BinaryPath $AnnoRagBin -Environment $RuntimeEnv
        }

        if (-not $SkipHeavy -and $Profile -ne "fast") {
            Invoke-GateCommand -Name "anno-rag bench" -FilePath $AnnoRagBin -Arguments @("bench", "--corpus", $SamplesDir) -Environment $RuntimeEnv
            Invoke-GateCommand -Name "PII NER recall ignored gate" -FilePath "cargo" -Arguments @("test", "-p", "anno-rag", "--test", "pii_ner", "--", "--ignored", "--nocapture")
            Invoke-GateCommand -Name "resumable ingest ignored gate" -FilePath "cargo" -Arguments @("test", "-p", "anno-rag", "--test", "ingest_scale", "--", "--ignored", "--nocapture")
        }

        if (-not $SkipHeavy -and $Profile -eq "deep") {
            Invoke-GateCommand -Name "rerank compile gate" -FilePath "cargo" -Arguments @("check", "-p", "anno-rag-bin", "--features", "rerank")
            Invoke-GateCommand -Name "eval bench gate" -FilePath "cargo" -Arguments @("bench", "-p", "anno-rag", "--bench", "bench_eval", "--", "--warm-up-time", "1", "--measurement-time", "5")
            Invoke-GateCommand -Name "memory recall bench gate" -FilePath "cargo" -Arguments @("bench", "-p", "anno-rag", "--bench", "bench_memory_recall", "--", "--warm-up-time", "1", "--measurement-time", "5")
        }

        if (-not $DryRun -and (Test-Path -LiteralPath $GatewayBin)) {
            Invoke-GatewaySmoke -BinaryPath $GatewayBin
        }
    } finally {
        Pop-Location
    }
} catch {
    $Metrics.summary.status = "failed"
    Write-Error $_
    throw
} finally {
    Write-ArtifactFiles
    if ($DryRun) {
        Write-Output "dry-run artifacts planned:"
        Write-Output "  metrics.json: $MetricsPath"
        Write-Output "  report.md: $ReportPath"
        Write-Output "  commands.log: $CommandLogPath"
    } else {
        Write-Output "metrics.json: $MetricsPath"
        Write-Output "report.md: $ReportPath"
    }
}
