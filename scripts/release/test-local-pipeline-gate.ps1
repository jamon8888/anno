[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptPath = $PSCommandPath
if (-not $ScriptPath) {
    $ScriptPath = $MyInvocation.MyCommand.Path
}

$ReleaseDir = Split-Path -Parent $ScriptPath
$RepoRoot = Split-Path -Parent (Split-Path -Parent $ReleaseDir)
$GateScript = Join-Path -Path $ReleaseDir -ChildPath "local-pipeline-gate.ps1"

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Condition,

        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

Assert-True -Condition (Test-Path -LiteralPath $GateScript -PathType Leaf) -Message "Missing scripts/release/local-pipeline-gate.ps1"

$Tokens = $null
$ParseErrors = $null
$Ast = [System.Management.Automation.Language.Parser]::ParseFile($GateScript, [ref]$Tokens, [ref]$ParseErrors)
Assert-True -Condition ($ParseErrors.Count -eq 0) -Message "PowerShell parse errors in local-pipeline-gate.ps1: $($ParseErrors -join '; ')"

$ParamBlock = $Ast.ParamBlock
Assert-True -Condition ($null -ne $ParamBlock) -Message "local-pipeline-gate.ps1 must declare an explicit param block"

$ParamNames = @($ParamBlock.Parameters | ForEach-Object { $_.Name.VariablePath.UserPath })
foreach ($Expected in @("Profile", "Corpus", "RunRoot", "SkipBuild", "SkipHeavy", "SkipOcr", "SkipMcp", "DryRun", "BuildTimeoutSecs")) {
    Assert-True -Condition ($ParamNames -contains $Expected) -Message "Missing parameter: $Expected"
}

$Text = Get-Content -LiteralPath $GateScript -Raw
foreach ($ExpectedText in @(
    "metrics.json",
    "report.md",
    "cargo check -p anno-rag --features embedded-ocr",
    "anno-rag bench",
    "anno-rag mcp",
    "anno-privacy-gateway boot smoke",
    "ANNO_RAG_DATA_DIR",
    "regex_pii_recall_meets_baseline"
)) {
    Assert-True -Condition ($Text.Contains($ExpectedText)) -Message "Missing expected gate marker: $ExpectedText"
}

$DryRunOutput = & $GateScript -DryRun -SkipHeavy -SkipOcr -SkipMcp 2>&1 | Out-String
Assert-True -Condition ($LASTEXITCODE -eq 0 -or $null -eq $LASTEXITCODE) -Message "Dry-run exited with code $LASTEXITCODE"
Assert-True -Condition ($DryRunOutput.Contains("local pipeline gate dry run")) -Message "Dry-run output did not identify itself"
Assert-True -Condition ($DryRunOutput.Contains("metrics.json")) -Message "Dry-run output must mention metrics.json"

Write-Output "local-pipeline-gate static tests passed"
