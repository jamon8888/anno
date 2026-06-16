[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$BinaryPath,

    [Parameter(Mandatory = $false)]
    [ValidateRange(1, 30)]
    [int]$Seconds = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ResolvedBinary = Resolve-Path -LiteralPath $BinaryPath
$StdoutPath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath ("anno-gateway-smoke-{0}.stdout.log" -f [System.Guid]::NewGuid())
$StderrPath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath ("anno-gateway-smoke-{0}.stderr.log" -f [System.Guid]::NewGuid())

function Stop-ProcessTree {
    param(
        [Parameter(Mandatory = $true)]
        [System.Diagnostics.Process]$TargetProcess
    )

    if ($TargetProcess.HasExited) {
        return
    }

    $TaskKill = Join-Path -Path $env:SystemRoot -ChildPath "System32\taskkill.exe"
    if (Test-Path -LiteralPath $TaskKill -PathType Leaf) {
        & $TaskKill /PID $TargetProcess.Id /T /F | Out-Null
    } else {
        Stop-Process -Id $TargetProcess.Id -Force
    }
}

$PreviousListen = [Environment]::GetEnvironmentVariable("ANNO_GATEWAY_LISTEN", "Process")
$Process = $null

try {
    [Environment]::SetEnvironmentVariable("ANNO_GATEWAY_LISTEN", "127.0.0.1:0", "Process")
    $Process = Start-Process `
        -FilePath $ResolvedBinary.Path `
        -WorkingDirectory (Get-Location).Path `
        -RedirectStandardOutput $StdoutPath `
        -RedirectStandardError $StderrPath `
        -WindowStyle Hidden `
        -PassThru

    if ($Process.WaitForExit($Seconds * 1000)) {
        Write-Error "Gateway exited early with code $($Process.ExitCode)." -ErrorAction Continue
        Write-Error "--- stdout ($StdoutPath) ---" -ErrorAction Continue
        Get-Content -LiteralPath $StdoutPath -ErrorAction SilentlyContinue | ForEach-Object { Write-Error $_ -ErrorAction Continue }
        Write-Error "--- stderr ($StderrPath) ---" -ErrorAction Continue
        Get-Content -LiteralPath $StderrPath -ErrorAction SilentlyContinue | ForEach-Object { Write-Error $_ -ErrorAction Continue }
        exit 1
    }

    Stop-ProcessTree -TargetProcess $Process
    $Process.WaitForExit()
    Write-Output "Gateway stayed alive for ${Seconds}s on ANNO_GATEWAY_LISTEN=127.0.0.1:0; smoke passed."
} finally {
    if ($null -ne $Process -and -not $Process.HasExited) {
        Stop-ProcessTree -TargetProcess $Process
        $Process.WaitForExit()
    }
    [Environment]::SetEnvironmentVariable("ANNO_GATEWAY_LISTEN", $PreviousListen, "Process")
}
