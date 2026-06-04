$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $inputObject = Read-AgentHarnessJsonFromStdin
    $command = Get-AgentHarnessCommandText -InputObject $inputObject
    $result = Test-AgentHarnessDangerousCommand -Command $command
    if ($result.Block) {
        $reason = "destructive command blocked by Hacienda agent harness: $($result.Reason)"
        [Console]::Error.WriteLine($reason)
        Write-AgentHarnessBlockJson -Reason $reason
        exit 2
    }
    exit 0
} catch {
    [Console]::Error.WriteLine("agent harness dangerous-command hook error: $($_.Exception.Message)")
    exit 1
}
