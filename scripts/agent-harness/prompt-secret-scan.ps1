$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $inputObject = Read-AgentHarnessJsonFromStdin
    $prompt = Get-AgentHarnessPromptText -InputObject $inputObject
    $result = Test-AgentHarnessSecretText -Text $prompt
    if ($result.Block) {
        $reason = "secret-like prompt content blocked by Hacienda agent harness: $($result.Reason)"
        [Console]::Error.WriteLine($reason)
        Write-AgentHarnessBlockJson -Reason $reason
        exit 2
    }
    exit 0
} catch {
    [Console]::Error.WriteLine("agent harness prompt-secret hook error: $($_.Exception.Message)")
    exit 1
}
